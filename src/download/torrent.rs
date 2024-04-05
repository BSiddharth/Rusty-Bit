use super::tracker;
use anyhow::{Context, Ok};
use futures_util::{future::join_all, SinkExt, StreamExt};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use serde_bencode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use rand::distributions::{Alphanumeric, DistString};
use sha1::{Digest, Sha1};

use crate::download::{
    peers::{PeerFrameCodec, PeerPieceMsgType, PeerRequestMsgType},
    tracker::{HandShake, TrackerResponse},
};
use crate::download::{
    peers::{PeerMsgTag, PeerMsgType},
    tracker::TrackerRequest,
};

use std::{
    clone,
    collections::HashMap,
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom},
    os::windows::prelude::FileExt,
    path::PathBuf,
    rc::Rc,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};
use std::{fmt, fs::File};
use std::{path::Path, usize};

fn calc_sha1_hash(piece_data: Vec<u8>) -> [u8; 20] {
    let mut piece_hasher = Sha1::new();
    piece_hasher.update(piece_data);
    let piece_hash = piece_hasher.finalize();
    Into::<[u8; 20]>::into(piece_hash)
}

#[derive(Debug)]
// using Vec beacuse we have no idea how large hash string can be
pub struct Hashes(Vec<[u8; 20]>);
struct HashesVisitor;

impl<'de> Visitor<'de> for HashesVisitor {
    type Value = Hashes;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a byte string whose length is multiple of 20")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if v.len() % 20 != 0 {
            return Err(E::custom(format!("length is {}", v.len())));
        }
        Result::Ok(Hashes(
            v.chunks_exact(20)
                .map(|x| x.try_into().expect("will be len 20"))
                .collect(),
        ))
    }
}

impl<'de> Deserialize<'de> for Hashes {
    fn deserialize<D>(deserializer: D) -> Result<Hashes, D::Error>
    where
        D: Deserializer<'de>,
    {
        // in bendy deserialize_bytes calls visit_borrowed_bytes which by default uses visit_bytes
        deserializer.deserialize_bytes(HashesVisitor)
    }
}

impl Serialize for Hashes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let data = &self.0;
        serializer.serialize_bytes(&data.concat())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TorrentFile {
    pub length: usize,
    path: Vec<String>,
}

// There are two possible forms:
//     one for the case of a 'single-file' torrent with no directory structure
//     one for the case of a 'multi-file' torrent
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum FileType {
    SingleFile { length: usize },
    MultiFile { files: Vec<TorrentFile> },
}

// Dictionary that describes the file(s) of the torrent.
#[derive(Debug, Deserialize, Serialize)]
pub struct Info {
    // suggested name for the file or the directory
    name: String,

    // number of bytes in each piece (integer)
    #[serde(rename = "piece length")]
    piece_length: usize,

    // string consisting of the concatenation of all 20-byte SHA1 hash values, one per piece.
    // Each piece has a corresponding SHA1 hash of the data contained within that piece.
    // These hashes are concatenated to form the pieces value in the above info dictionary.
    // Note that this is not a list but rather a single string in the bencoded form.
    // The length of the string must be a multiple of 20.
    // Instead of string using Vec<u8> beacause it is possible that bytes are not valid UTF-8.
    pieces: Hashes,

    #[serde(flatten)]
    pub file_type: FileType,
}

// The content of a Torrent is a bencoded dictionary, containing the keys listed below. All character string values are UTF-8 encoded.
// No optional field included for now.
#[derive(Debug, Deserialize, Serialize)]
pub struct Torrent {
    pub info: Info,

    // The announce URL of the tracker (string)
    pub announce: String,
}

#[derive(Debug)]
struct PieceLocationMap {
    path: String,
    offset: usize,
    length: usize,
}

impl Torrent {
    pub fn calc_hash(&mut self) -> anyhow::Result<[u8; 20]> {
        let mut hasher = Sha1::new();
        hasher.update(
            serde_bencode::to_bytes::<Info>(&self.info)
                .context("Metainfo file's Info conversion to bytes")?,
        );
        let info_hash = hasher.finalize().into();
        Ok(info_hash)
    }

    // reserve space for files to be downloaded
    fn reserve_space(&self, download_directory_path: &str) {
        match &self.info.file_type {
            FileType::SingleFile { length } => {
                let file_path = Path::new(download_directory_path).join(&self.info.name);
                if !file_path.exists() {
                    std::fs::write(file_path, vec![0; *length]).unwrap();
                }
            }
            FileType::MultiFile { files } => {
                for file in files {
                    let mut file_path = PathBuf::from(download_directory_path);
                    for sub_directory in &file.path {
                        file_path.push(sub_directory);
                    }

                    if !file_path.exists() {
                        let parent_path = file_path.parent().expect("There has to be a parent");
                        std::fs::create_dir_all(parent_path).unwrap();
                        std::fs::write(file_path, vec![0; file.length]).unwrap();
                    }
                }
            }
        }
    }

    // generate a mapping of piece to its corresponding files
    fn genereate_piece_mapping(
        &self,
        total_pieces_to_download: usize,
        torrent_data_len: usize,
        download_directory_path: &str,
    ) -> anyhow::Result<HashMap<usize, Vec<PieceLocationMap>>> {
        let mut piece_mapping: HashMap<usize, Vec<PieceLocationMap>> = HashMap::new();
        let mut file_vec = Vec::new();

        // generate a vec of file details
        match &self.info.file_type {
            FileType::SingleFile { length } => {
                let file_name = &self.info.name;
                let mut path = PathBuf::from(download_directory_path);
                path.push(file_name);
                file_vec.push(PieceLocationMap {
                    path: path.to_str().unwrap().to_string(),
                    offset: 0,
                    length: *length,
                });
            }
            FileType::MultiFile { files } => {
                for file in files {
                    let mut path = PathBuf::from(download_directory_path);
                    for p in &file.path {
                        path.push(p);
                    }
                    file_vec.push(PieceLocationMap {
                        path: path.to_str().unwrap().to_string(),
                        offset: 0,
                        length: file.length,
                    });
                }
            }
        }

        let mut file_vec_iter = file_vec.into_iter();
        let piece_length = self.info.piece_length;

        let mut current_file_details = file_vec_iter.next().unwrap();
        let mut current_offset = 0;
        let mut data_examined = 0;

        for piece_index in 0..total_pieces_to_download {
            let mut piece_data_read = 0;
            let piece_data_to_read = std::cmp::min(piece_length, torrent_data_len - data_examined);
            let mut piece_location_vec: Vec<PieceLocationMap> = Vec::new();

            while piece_data_read != piece_data_to_read {
                if current_file_details.length
                    >= current_offset + (piece_data_to_read - piece_data_read)
                {
                    let data_to_read_in_this_iter = piece_data_to_read - piece_data_read;
                    piece_location_vec.push(PieceLocationMap {
                        path: current_file_details.path.clone(),
                        offset: current_offset,
                        length: data_to_read_in_this_iter,
                    });
                    current_offset += data_to_read_in_this_iter;
                    piece_data_read += data_to_read_in_this_iter;
                    data_examined += data_to_read_in_this_iter;
                } else {
                    let data_to_read_in_this_iter = current_file_details.length - current_offset;
                    if data_to_read_in_this_iter != 0 {
                        piece_location_vec.push(PieceLocationMap {
                            path: current_file_details.path.clone(),
                            offset: current_offset,
                            length: data_to_read_in_this_iter,
                        });
                        piece_data_read += data_to_read_in_this_iter;
                        data_examined += data_to_read_in_this_iter;
                    }

                    if torrent_data_len != data_examined {
                        current_file_details = file_vec_iter.next().unwrap();
                    }
                    current_offset = 0;
                }
            }

            piece_mapping.insert(piece_index, piece_location_vec);
        }
        Ok(piece_mapping)
    }

    fn pieces_to_be_downloaded(
        &self,
        total_pieces_to_download: usize,
        // piece_mapping: HashMap<usize, Vec<PieceLocationMap>>,
        piece_mapping: Arc<HashMap<usize, Vec<PieceLocationMap>>>,
    ) -> anyhow::Result<Vec<usize>> {
        let mut to_be_downloaded_pieces: Vec<usize> = Vec::new();
        let mut current_path = &piece_mapping[&0][0].path;
        let mut current_file_handler = File::open(current_path).unwrap();

        for piece_index in 0..total_pieces_to_download {
            let buffer_len = piece_mapping[&piece_index]
                .iter()
                .fold(0, |acc, x| acc + x.length);

            let mut buf: Vec<u8> = Vec::with_capacity(buffer_len);

            for piece_location_map in piece_mapping[&piece_index].iter() {
                let mut sub_buf: Vec<u8> = Vec::with_capacity(piece_location_map.length);
                if &piece_location_map.path != current_path {
                    current_path = &piece_location_map.path;
                    current_file_handler = File::open(&piece_location_map.path).unwrap();
                }
                current_file_handler.seek(SeekFrom::Start(piece_location_map.offset as u64))?;
                current_file_handler.read_exact(&mut sub_buf).unwrap();
                buf.append(&mut sub_buf);
            }
            if calc_sha1_hash(buf) != self.info.pieces.0[piece_index] {
                to_be_downloaded_pieces.push(piece_index);
            }
        }

        Ok(to_be_downloaded_pieces)
    }

    pub async fn start_download(&mut self) -> anyhow::Result<()> {
        // Create a directory if it does not already exist
        let download_directory_path = format!(
            "Downloaded/{}",
            &self
                .info
                .name
                .split('.')
                .next()
                .context("Removing extension from the torrent name")?
        );
        std::fs::create_dir_all(&download_directory_path)
            .context("Creating directory to store the downloaded content")?;

        // reserve space for files to be downloaded
        self.reserve_space(&download_directory_path);

        let total_pieces_to_download = self.info.pieces.0.len();

        let torrent_data_len: usize = match self.info.file_type {
            FileType::SingleFile { length } => length,
            FileType::MultiFile { ref files } => files.iter().map(|file| file.length).sum(),
        };

        println!(
            "Total bytes to download: {}\nTotal pieces to download: {}\n",
            torrent_data_len, total_pieces_to_download
        );

        // generate a mapping of piece to its corresponding files
        let piece_mapping = Arc::new(self.genereate_piece_mapping(
            total_pieces_to_download,
            torrent_data_len,
            &download_directory_path,
        )?);

        // find out the completion status
        let pieces_to_download = Arc::new(Mutex::new(
            self.pieces_to_be_downloaded(total_pieces_to_download, piece_mapping.clone())?,
        ));

        println!("pieces to download are {pieces_to_download:?}");

        let info_hash = self.calc_hash().context("Calculate metainfo hash")?;

        let announce = &self.announce;
        println!(
            "Starting download now, trying to contact tracker at {}\n",
            announce
        );

        let peer_id = Alphanumeric.sample_string(&mut rand::thread_rng(), 20);
        let tracker_request = TrackerRequest::new(info_hash, torrent_data_len, &peer_id);
        let url = tracker_request.url(announce);

        // let response = reqwest::Client::new()
        //     .get(url)
        //     .send()
        //     .await
        //     .with_context(|| format!("Requesting tracker {}", announce))?;

        let response = reqwest::get(url)
            .await
            .with_context(|| format!("Requesting tracker {}", announce))?;

        let tracker_reponse: TrackerResponse =
            serde_bencode::from_bytes(&response.bytes().await.with_context(|| {
                format!("Converting tracker's ({}) response to bytes", announce)
            })?)
            .with_context(|| {
                format!(
                    "Converting tracker's ({}) response bytes to TrackerResponse",
                    announce
                )
            })?;

        match tracker_reponse.tracker_response_type {
            tracker::TrackerResponseType::Success {
                complete: _,
                incomplete: _,
                interval: _,
                peers,
                tracker_id: _,
            } => {
                println!("Connected to the tracker {announce}");

                let peer_list: Vec<String> = peers
                    .0
                    .iter()
                    .map(|peer_info| format!("{}:{}", peer_info.ip_addr, peer_info.port))
                    .collect();
                println!("All the available peers are: {peer_list:?}");
                println!("Connecting to the peers");

                let mut handle_vec = Vec::new();

                let handshake = HandShake::new(info_hash, peer_id.as_bytes().try_into().unwrap());
                let encoded_handshake = Arc::new(bincode::serialize(&handshake).unwrap());

                let file_handle_mapping: Arc<Mutex<HashMap<String, File>>> =
                    Arc::new(Mutex::new(HashMap::new()));

                let pieces_hash = &self.info.pieces.0;
                for peer in peer_list {
                    let encoded_handshake = encoded_handshake.clone();
                    let pieces_to_download = pieces_to_download.clone();
                    let file_handle_mapping = file_handle_mapping.clone();
                    let piece_length = self.info.piece_length;
                    let piece_mapping = piece_mapping.clone();
                    let pieces_hash = pieces_hash.clone();
                    handle_vec.push(tokio::spawn(async move {
                        let mut stream = tokio::net::TcpStream::connect(peer)
                            .await
                            .context("Connecting with peer")
                            .unwrap();

                        // send handshake
                        stream.write_all(&encoded_handshake.clone()).await.unwrap();
                        let mut response = vec![0_u8; encoded_handshake.len()];
                        stream.read_exact(&mut response).await.unwrap();

                        let _response_handshake: HandShake =
                            bincode::deserialize(&response).unwrap();

                        // println!("pstrlen: {}", response_handshake.pstrlen);
                        // println!(
                        //     "pstr: {}",
                        //     String::from_utf8(response_handshake.pstr.to_vec()).unwrap()
                        // );
                        // println!("peer_id: {:x?}", &response_handshake.peer_id.to_vec());
                        // println!("reserved bytes: {:?}", &response_handshake.reserved);

                        let mut framed = tokio_util::codec::Framed::new(stream, PeerFrameCodec);

                        let new_frame = framed.next().await.unwrap().unwrap(); // bitfield msg
                                                                               // println!("next frame type is {new_frame:?}",);

                        // println!("Sending interested frame");
                        framed
                            .send(PeerMsgType::new(PeerMsgTag::Interested, Vec::new()))
                            .await
                            .unwrap();

                        let new_frame = framed.next().await.unwrap().unwrap();
                        // println!("next frame type is {new_frame:?}");

                        let max_request_block_size = 2_usize.pow(13);

                        loop {
                            let piece_index = pieces_to_download.lock().unwrap().pop();
                            if piece_index.is_none() {
                                break;
                            }

                            let piece_index = piece_index.unwrap();
                            // println!("Piece index is **** {piece_index}");

                            let piece_to_download_len = if piece_index
                                != total_pieces_to_download - 1
                            {
                                piece_length
                            } else {
                                torrent_data_len - (piece_length * (total_pieces_to_download - 1))
                            };
                            // println!("dltd {piece_to_download_len}");

                            let mut piece_data: Vec<u8> = Vec::new();
                            piece_data.reserve_exact(piece_to_download_len);

                            let mut piece_downloaded_len: usize = 0;

                            while piece_to_download_len != piece_downloaded_len {
                                // println!("downloading piece {}", piece_index);

                                let this_block_data_len = std::cmp::min(
                                    piece_to_download_len - piece_downloaded_len,
                                    max_request_block_size,
                                );
                                // println!("tbdl {this_block_data_len}");

                                let peer_msg_req_bytes = PeerRequestMsgType::new(
                                    piece_index as u32,
                                    piece_downloaded_len as u32,
                                    this_block_data_len as u32,
                                )
                                .to_bytes();

                                framed
                                    .send(PeerMsgType::new(
                                        PeerMsgTag::Request,
                                        peer_msg_req_bytes.to_vec(),
                                    ))
                                    .await
                                    .unwrap();
                                let new_frame = framed.next().await.unwrap().unwrap();
                                assert_eq!(&PeerMsgTag::Piece, new_frame.tag());
                                piece_data.append(
                                    &mut PeerPieceMsgType::from_bytes(new_frame.data()).block(),
                                );
                                piece_downloaded_len += this_block_data_len;
                            }
                            assert_eq!(piece_to_download_len, piece_data.len());

                            let piece_hash = calc_sha1_hash(piece_data.clone());
                            assert_eq!(pieces_hash[piece_index], piece_hash);

                            let file_paths_details = &piece_mapping[&piece_index];
                            let mut handle_mapping = file_handle_mapping.lock().unwrap();
                            let mut piece_data_pointer = 0;
                            // println!("{}", std::str::from_utf8(&piece_data).unwrap());
                            for file_path_detail in file_paths_details {
                                if !handle_mapping.contains_key(&file_path_detail.path) {
                                    handle_mapping.insert(
                                        file_path_detail.path.clone(),
                                        OpenOptions::new()
                                            .write(true)
                                            .open(&file_path_detail.path)
                                            .unwrap(),
                                    );
                                }

                                let handle = &handle_mapping[&file_path_detail.path];
                                let _ = handle.seek_write(
                                    &piece_data[piece_data_pointer
                                        ..piece_data_pointer + file_path_detail.length],
                                    file_path_detail.offset as u64,
                                );
                                piece_data_pointer += file_path_detail.length;
                            }
                        }
                    }));
                }

                // // Create a file
                // let mut data_file = File::create(format!(
                //     "C:/Users/SIDDHARTH/Desktop/torrent download/{}",
                //     self.info.name.clone()
                // ))
                // .expect("creation failed");

                // Write contents to the file
                // data_file.write(&final_bytes).expect("write failed");

                join_all(handle_vec).await;
                println!("Downloaded file {}", self.info.name.clone());
            }
            tracker::TrackerResponseType::Failure { failure_reason } => {
                println!("Tracker {announce} could not be connected due to: {failure_reason}\n");
            }
        }
        Ok(())
    }
}

use super::tracker;
use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
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

use std::io::{Read, Seek, SeekFrom, Write};
use std::{collections::HashSet, path::Path, usize};
use std::{fmt, fs::File};

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

    fn pieces_to_be_downloaded(
        &self,
        directory_path: &str,
        total_pieces_to_download: usize,
    ) -> anyhow::Result<HashSet<usize>> {
        let mut to_be_downloaded_pieces: HashSet<usize> = HashSet::new();
        let piece_length = self.info.piece_length;
        match &self.info.file_type {
            FileType::SingleFile { length } => {
                let file_name = &self.info.name;
                let path = Path::new(directory_path).join(file_name);

                if path.exists() {
                    let mut current_piece_being_examined = 0;
                    let mut data_examined = 0;
                    let mut file_handler =
                        File::open(path).context("Opening handle to the single torrent file")?;
                    while data_examined <= *length {
                        let data_to_examine_len =
                            std::cmp::min(piece_length, *length - data_examined);
                        let mut buf = Vec::with_capacity(data_to_examine_len);
                        let _ = file_handler.seek(SeekFrom::Start(data_examined as u64));
                        file_handler
                            .read_exact(&mut buf)
                            .expect("Error reading file");
                        if self.info.pieces.0[current_piece_being_examined] != calc_sha1_hash(buf) {
                            to_be_downloaded_pieces.insert(current_piece_being_examined);
                        }
                        data_examined += data_to_examine_len;
                        current_piece_being_examined += 1;
                    }
                } else {
                    for piece in 0..=total_pieces_to_download {
                        to_be_downloaded_pieces.insert(piece);
                    }
                }
            }
            FileType::MultiFile { files } => todo!(),
        }

        return Ok(to_be_downloaded_pieces);
    }

    pub async fn start_download(&mut self) -> anyhow::Result<()> {
        // Create a directory if it does not already exist
        let directory_path = format!(
            "Downloaded/{}",
            &self
                .info
                .name
                .split(".")
                .next()
                .context("Removing extension from the torrent name")?
        );
        std::fs::create_dir_all(&directory_path)
            .context("Creating directory to store the downloaded content")?;

        let total_pieces_to_download = self.info.pieces.0.len();

        // find out the completion status
        let to_be_downloaded_pieces =
            self.pieces_to_be_downloaded(&directory_path, total_pieces_to_download)?;

        let mut torrent_data_len: usize = match self.info.file_type {
            FileType::SingleFile { length } => length,
            FileType::MultiFile { ref files } => files.iter().map(|file| file.length).sum(),
        };

        println!(
            "Total bytes to download: {}\nTotal pieces to download: {}",
            torrent_data_len, total_pieces_to_download
        );

        let info_hash = self.calc_hash().context("Calculate metainfo hash")?;

        let announce = &self.announce;
        println!(
            "Starting download now, trying to contact tracker at {}",
            announce
        );

        let peer_id = Alphanumeric.sample_string(&mut rand::thread_rng(), 20);
        let tracker_request = TrackerRequest::new(info_hash, torrent_data_len, &peer_id);
        let url = tracker_request.url(announce);
        let response = reqwest::Client::new()
            .get(url)
            .send()
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
                    .map(|peer_info| {
                        format!("{}:{}", peer_info.ip_addr, peer_info.port.to_string())
                    })
                    .collect();
                println!("All the available peers are: {peer_list:?}");
                println!("Connecting to the first peer");

                let handshake = HandShake::new(info_hash, peer_id.as_bytes().try_into().unwrap());
                let encoded = bincode::serialize(&handshake).unwrap();
                let mut stream = tokio::net::TcpStream::connect(&peer_list[0])
                    .await
                    .context("Connecting with peer")?;
                // .context("Connecting with peer")?;
                stream.write_all(&encoded).await?;
                let mut response = [0 as u8; 68]; // TODO: Remove the hardcoded value
                stream.read_exact(&mut response).await?;

                let response_handshake: HandShake = bincode::deserialize(&response)?;

                println!("pstrlen: {}", response_handshake.pstrlen);
                println!(
                    "pstr: {}",
                    String::from_utf8(response_handshake.pstr.to_vec()).unwrap()
                );
                println!("peer_id: {:x?}", &response_handshake.peer_id.to_vec());
                println!("reserved bytes: {:?}", &response_handshake.reserved);

                let mut framed = tokio_util::codec::Framed::new(stream, PeerFrameCodec);

                let new_frame = framed.next().await.unwrap().unwrap();
                println!("next frame type is {new_frame:?}",);

                println!("Sending interested frame");
                let _ = framed
                    .send(PeerMsgType::new(PeerMsgTag::Interested, Vec::new()))
                    .await
                    .unwrap();

                let new_frame = framed.next().await.unwrap().unwrap();
                println!("next frame type is {new_frame:?}");

                let mut final_bytes: Vec<u8> = Vec::new();
                final_bytes.reserve_exact(torrent_data_len);

                let max_request_block_size = 2_usize.pow(13);
                for piece_index in 0..total_pieces_to_download as usize {
                    let piece_to_download_len =
                        std::cmp::min(torrent_data_len, self.info.piece_length);
                    println!("dltd {piece_to_download_len}");

                    let mut piece_data: Vec<u8> = Vec::new();
                    piece_data.reserve_exact(piece_to_download_len);

                    let mut piece_downloaded_len: usize = 0;

                    while piece_to_download_len != piece_downloaded_len {
                        println!("downloading piece {}", piece_index);

                        let this_block_data_len = std::cmp::min(
                            piece_to_download_len - piece_downloaded_len,
                            max_request_block_size,
                        );
                        println!("tbdl {this_block_data_len}");

                        let peer_msg_req_bytes = PeerRequestMsgType::new(
                            piece_index as u32,
                            piece_downloaded_len as u32,
                            this_block_data_len as u32,
                        )
                        .to_bytes();

                        let _ = framed
                            .send(PeerMsgType::new(
                                PeerMsgTag::Request,
                                peer_msg_req_bytes.to_vec(),
                            ))
                            .await
                            .unwrap();
                        let new_frame = framed.next().await.unwrap().unwrap();
                        assert_eq!(&PeerMsgTag::Piece, new_frame.tag());
                        piece_data
                            .append(&mut PeerPieceMsgType::from_bytes(new_frame.data()).block());
                        piece_downloaded_len += this_block_data_len;
                    }
                    assert_eq!(piece_to_download_len, piece_data.len());
                    let piece_hash = calc_sha1_hash(piece_data.clone());

                    assert_eq!(self.info.pieces.0[piece_index], piece_hash);
                    torrent_data_len -= piece_to_download_len;
                    final_bytes.append(&mut piece_data);
                }
                // Create a file
                let mut data_file = File::create(format!(
                    "C:/Users/SIDDHARTH/Desktop/torrent download/{}",
                    self.info.name.clone()
                ))
                .expect("creation failed");

                // Write contents to the file
                data_file.write(&final_bytes).expect("write failed");

                println!("Downloaded file {}", self.info.name.clone());
            }
            tracker::TrackerResponseType::Failure { failure_reason } => {
                println!("Tracker {announce} could not be connected due to: {failure_reason}");
            }
        }
        Ok(())
    }
}

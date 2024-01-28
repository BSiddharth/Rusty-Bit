use super::tracker;
use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use serde_bencode;
use std::{fmt, usize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use rand::distributions::{Alphanumeric, DistString};
use sha1::{Digest, Sha1};

use crate::download::{
    peers::{PeerFrameCodec, PeerRequestMsgType},
    tracker::{HandShake, TrackerResponse},
};
use crate::download::{
    peers::{PeerMsgTag, PeerMsgType},
    tracker::TrackerRequest,
};

// using Vec beacuse we have no idea how large can hash string be
#[derive(Debug)]
struct Hashes(Vec<[u8; 20]>);
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
pub struct File {
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
    MultiFile { files: Vec<File> },
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

    #[serde(skip)]
    pub info_hash: Option<[u8; 20]>,
}

impl Torrent {
    pub fn calc_hash(&mut self) -> anyhow::Result<[u8; 20]> {
        match self.info_hash {
            Some(info_hash) => Ok(info_hash),
            None => {
                let mut hasher = Sha1::new();
                hasher.update(
                    serde_bencode::to_bytes::<Info>(&self.info)
                        .context("Info hash could not calculated")?,
                );
                let info_hash = hasher.finalize().into();
                self.info_hash = Some(info_hash);
                Ok(info_hash)
            }
        }
    }

    pub async fn start_download(&mut self) -> anyhow::Result<()> {
        let total_pieces_to_download = self.info.pieces.0.len();
        // let total_pieces_to_download = (self.info.pieces.0.len() as f64 / 20.0).ceil();
        // cannot do this because query uses urlencoded which cannot Serialize [u8] !!
        // let client = reqwest::blocking::Client::new();
        // let response = client.get(base_url).query(self).send();
        let torrent_data_len: usize = match self.info.file_type {
            FileType::SingleFile { length } => length,
            FileType::MultiFile { ref files } => files.iter().map(|file| file.length).sum(),
        };
        println!(
            "Total bytes to download: {}\nTotal pieces to download: {}",
            torrent_data_len, total_pieces_to_download
        );
        let info_hash = self.calc_hash().context("could not calculate hash")?;
        let peer_id = Alphanumeric.sample_string(&mut rand::thread_rng(), 20);
        let tracker_request = TrackerRequest::new(info_hash, torrent_data_len, &peer_id);
        let url = tracker_request.url(&self.announce);
        let response = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .context("Request to tracker failed")?;
        // println!("{:?}", response.text());
        // println!("response: {:?}", response.text());
        let tracker_reponse: TrackerResponse = serde_bencode::from_bytes(
            &response
                .bytes()
                .await
                .context("could not convert tracker response to bytes")?,
        )
        .context("could not convert tracker response bytes to TrackerResponse")?;

        match tracker_reponse.tracker_response_type {
            tracker::TrackerResponseType::Success {
                complete: _,
                incomplete: _,
                interval: _,
                peers,
                tracker_id: _,
            } => {
                println!("Connected to the tracker");

                let peer_list: Vec<String> = peers
                    .0
                    .iter()
                    .map(|peer_info| {
                        format!("{}:{}", peer_info.ip_addr, peer_info.port.to_string())
                    })
                    .collect();
                println!("All the available peers are: {peer_list:?}");
                println!("Connecting to the first peer");

                let handshake = HandShake::new(
                    self.info_hash.unwrap(),
                    peer_id.as_bytes().try_into().unwrap(),
                );
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

                let max_request_block_size = 2_usize.pow(13);
                for piece_index in 0..total_pieces_to_download as usize {
                    let mut data_left_to_download_in_the_piece = self.info.piece_length;
                    let mut block_count: u32 = 0;
                    while data_left_to_download_in_the_piece != 0 {
                        println!(
                            "downloading piece {} and block {}",
                            piece_index, block_count
                        );
                        let this_block_data_len = std::cmp::min(
                            data_left_to_download_in_the_piece,
                            max_request_block_size,
                        );

                        let peer_msg_req_bytes = PeerRequestMsgType::new(
                            piece_index as u32,
                            block_count,
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
                        println!("next frame type is {new_frame:?}",);
                        data_left_to_download_in_the_piece -= this_block_data_len;
                        block_count += 1;
                    }
                }
            }
            tracker::TrackerResponseType::Failure { failure_reason } => {
                println!("tracker could not be connected due to: {failure_reason}");
            }
        }
        Ok(())
    }
}

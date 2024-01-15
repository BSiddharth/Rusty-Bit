use anyhow::{Context, Ok};
use reqwest::blocking::Response;
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use serde_bencode;
use std::{fmt, usize};

use sha1::{Digest, Sha1};

use crate::download::tracker::TrackerRequest;
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

    pub fn start_download(&mut self) -> anyhow::Result<Response> {
        // cannot do this because query uses urlencoded which cannot Serialize [u8] !!
        // let client = reqwest::blocking::Client::new();
        // let response = client.get(base_url).query(self).send();

        let torrent_data_len: usize = match self.info.file_type {
            FileType::SingleFile { length } => length,
            FileType::MultiFile { ref files } => files.iter().map(|file| file.length).sum(),
        };
        let info_hash = self.calc_hash().context("could not calculate hash")?;
        let tracker_request = TrackerRequest::new(info_hash, torrent_data_len);
        let url = tracker_request.url(&self.announce);
        let response = reqwest::blocking::get(url)?;
        // println!("{:?}", response.text());
        Ok(response)
    }
}

use crate::helper::read_string;
use anyhow::bail;
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{
    fmt, fs,
    io::{self, ErrorKind, Write},
};

/*
 * This function is responsible for converting the data in bencoded file into rust datatype.
*/
fn decode_bencoded_file(file_path: String) -> anyhow::Result<Torrent> {
    println!("Trying to read file {file_path}");
    let file_data = fs::read(&file_path);
    match file_data {
        Ok(file_data_vec) => {
            println!("Decoding bencoded file {file_path}");
            let decoder_result = bendy::serde::from_bytes::<Torrent>(&file_data_vec);
            match decoder_result {
                Ok(torrent_data) => Ok(torrent_data),
                Err(_) => {
                    println!("File could not be decoded!");
                    bail!("File could not be decoded!")
                }
            }
        }
        Err(e) => {
            if e.kind() == ErrorKind::NotFound {
                println!("File path does not exist, check the file path again!!");
            } else {
                println!("Could not read the file!!");
            }
            Err(e.into())
        }
    }
}

// using Vec beacuse we have no idea how large can hash string be
#[derive(Debug, Serialize)]
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
        Ok(Hashes(
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

#[derive(Debug, Deserialize, Serialize)]
struct File {
    length: usize,
    path: Vec<String>,
}

// There are two possible forms:
//     one for the case of a 'single-file' torrent with no directory structure
//     one for the case of a 'multi-file' torrent
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum FileType {
    SingleFile { length: usize },
    MultiFile { files: Vec<File> },
}

// Dictionary that describes the file(s) of the torrent.
#[derive(Debug, Deserialize, Serialize)]
struct Info {
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
    file_type: FileType,
}

// The content of a Torrent is a bencoded dictionary, containing the keys listed below. All character string values are UTF-8 encoded.
// No optional field included for now.
#[derive(Debug, Deserialize, Serialize)]
struct Torrent {
    info: Info,

    // The announce URL of the tracker (string)
    announce: String,
}

/*
 * This function downloads torrent resource using the .torrent file
*/
pub fn download_using_file() -> anyhow::Result<()> {
    print!("You chose to download using .torrent file, provide the file path: ");
    io::stdout().flush().expect("Couldn't flush stdout");

    let file_path = read_string();
    println!();
    let decoded_file_data = decode_bencoded_file(file_path)?;

    // Console output is handled by the decode_bencoded_file function so no need to take any action
    // in case of faiure.
    let announce = decoded_file_data.announce;
    println!("Starting download now, trying to contact {}", announce);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::download2::Torrent;

    #[test]
    fn bendy_from_bytes_success() {
        let file_data = fs::read("torrent sample/sample.torrent").unwrap();
        bendy::serde::from_bytes::<Torrent>(&file_data).unwrap();

        let file_data = fs::read("torrent sample/am.torrent").unwrap();
        bendy::serde::from_bytes::<Torrent>(&file_data).unwrap();

        let file_data = fs::read("torrent sample/example.torrent").unwrap();
        bendy::serde::from_bytes::<Torrent>(&file_data).unwrap();

        let file_data = fs::read("torrent sample/test.torrent").unwrap();
        bendy::serde::from_bytes::<Torrent>(&file_data).unwrap();

        // cannot check if orignal bytes are equal to encoded bytes since I skip some all optional field
        // let encoder_result = bendy::serde::to_bytes::<Torrent>(&decoder_result).unwrap();
        // assert_eq!(file_data.len(), encoder_result.len());
    }
}

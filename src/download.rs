use crate::{download::tracker::TrackerResponse, helper::read_string};
use anyhow::{bail, Context};
use std::{
    fs,
    io::{self, ErrorKind, Write},
};
mod torrent;
mod tracker;
use serde_bencode;
use torrent::Torrent;

/*
 * This function is responsible for converting the data in bencoded file into rust datatype.
*/
fn decode_bencoded_file(file_path: String) -> anyhow::Result<Torrent> {
    println!("Trying to read file {file_path}");
    let file_data = fs::read(&file_path);
    match file_data {
        Ok(file_data_vec) => {
            println!("Decoding bencoded file {file_path}");
            let decoder_result = serde_bencode::from_bytes::<Torrent>(&file_data_vec);
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

/*
 * This function downloads torrent resource using the .torrent file
*/
pub fn download_using_file() -> anyhow::Result<()> {
    print!("You chose to download using .torrent file, provide the file path: ");
    io::stdout().flush().expect("Couldn't flush stdout");

    let file_path = read_string();
    println!();
    let mut decoded_file_data = decode_bencoded_file(file_path)?;
    // Console output is handled by the decode_bencoded_file function so no need to take any action in case of faiure.

    let announce = &decoded_file_data.announce;
    println!("Starting download now, trying to contact {}", announce);

    let response = decoded_file_data
        .start_download()
        .context("Could not start download")?;

    // println!("response: {:?}", response.text());
    let tracker_reponse: TrackerResponse = serde_bencode::from_bytes(
        // r"d8:completei3e10:incompletei3e8:intervali60e12:min intervali60e5:peers18:�>RY�\u{e}��!M�\u{b}�>U\u{14}�!e"
        // .as_bytes(),
        &response
            .bytes()
            .context("could not convert response to bytes")?,
    )
    .context("could not convert response bytes to TrackerResponse")?;

    println!("{tracker_reponse:?}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::Torrent;

    #[test]
    fn bendy_from_bytes_success() {
        let file_data = fs::read("torrent sample/sample.torrent").unwrap();
        let deserialized_data = serde_bencode::from_bytes::<Torrent>(&file_data).unwrap();
        let _ = serde_bencode::to_bytes(&deserialized_data);

        let file_data = fs::read("torrent sample/am.torrent").unwrap();
        let deserialized_data = serde_bencode::from_bytes::<Torrent>(&file_data).unwrap();
        let _ = serde_bencode::to_bytes(&deserialized_data);

        let file_data = fs::read("torrent sample/example.torrent").unwrap();
        let deserialized_data = serde_bencode::from_bytes::<Torrent>(&file_data).unwrap();
        let _ = serde_bencode::to_bytes(&deserialized_data);

        let file_data = fs::read("torrent sample/test.torrent").unwrap();
        let deserialized_data = serde_bencode::from_bytes::<Torrent>(&file_data).unwrap();
        let _ = serde_bencode::to_bytes(&deserialized_data);

        // cannot check if orignal bytes are equal to encoded bytes since I skip some all optional field
        // let encoder_result = serde_bencode::to_bytes::<Torrent>(&decoder_result).unwrap();
        // assert_eq!(file_data.len(), encoder_result.len());
    }
}

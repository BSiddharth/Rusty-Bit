use crate::{
    download::{torrent::Info, tracker::TrackerRequest},
    helper::read_string,
};
use anyhow::{bail, Context};
use rand::RngCore;
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::{self, ErrorKind, Write},
};
mod torrent;
mod tracker;
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

    let mut hasher = Sha1::new();
    hasher.update(
        bendy::serde::to_bytes::<Info>(&decoded_file_data.info)
            .context("Info hash could not calculated")?,
    );
    let info_hash = hasher.finalize();
    println!("So the hash is {:?}", info_hash);

    let mut peer_id = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut peer_id);
    println!("{:?}", peer_id);

    let torrent_data_len = match decoded_file_data.info.file_type {
        torrent::FileType::SingleFile { length } => length,
        torrent::FileType::MultiFile { files } => files.iter().map(|file| file.length).sum(),
    };

    let tracker_request = TrackerRequest {
        info_hash: info_hash.into(),
        peer_id,
        port: 6888,
        uploaded: 0,
        downloaded: 0,
        left: torrent_data_len,
        compact: 1,
        no_peer_id: 6,
        event: tracker::Event::STARTED,
    };

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::Torrent;

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

use crate::helper::read_string;
use anyhow::bail;
use std::{
    char, fs,
    io::{self, ErrorKind, Write},
    usize,
};

#[derive(PartialEq, Debug)]
enum BencodeType {
    Bstring(String),
    Bvec(Vec<String>),
    Bmap(Box<BencodeType>),
    Bint(u64),
}

fn recursive_bencode_decoder(data: &Vec<u8>) -> anyhow::Result<BencodeType> {
    let mut string_len_string: String = String::new();
    let mut index: usize = 0;

    let mut data_char: char = data[index] as char;
    if data_char.is_ascii_digit() {
        while data_char != ':' {
            string_len_string.push(data_char);
            index += 1;
            data_char = data[index] as char;
        }
        return Ok(BencodeType::Bstring(String::from_utf8(
            data[index + 1..index + 1 + string_len_string.parse::<usize>()?].to_vec(),
        )?));
    }
    bail!("Could not parse the bencoded data properly!")
}

fn decode_bencoded_file(file_path: String) -> anyhow::Result<BencodeType> {
    println!("Trying to read file {file_path}");
    let file_data = fs::read(&file_path);
    match file_data {
        Ok(file_data_vec) => {
            println!("Decoding bencoded file {file_path}");
            print!("{}", String::from_utf8_lossy(&file_data_vec));
            Ok(BencodeType::Bint(0))
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

pub fn download_using_file() -> anyhow::Result<()> {
    print!("You chose to download using .torrent file, provide the file path: ");
    io::stdout().flush().expect("Couldn't flush stdout");
    let file_path = read_string();
    decode_bencoded_file(file_path)?;
    Ok(())
}

#[cfg(test)]
mod download_test {
    use crate::download::{decode_bencoded_file, recursive_bencode_decoder, BencodeType};

    #[test]
    fn check_decode_becoded_file_fails() {
        let file_path: String = "non exisitng file path".to_string();
        assert!(decode_bencoded_file(file_path).is_err());
    }

    #[test]
    fn check_decode_becoded_file_passes() {
        let file_path: String = r"torrent sample\sample.torrent".to_string();
        assert!(decode_bencoded_file(file_path).is_ok());
    }

    #[test]
    fn check_recursive_bencode_decoder_passes() {
        let str_vec: &Vec<u8> = &String::from("11:Test String").into_bytes();
        assert_eq!(
            recursive_bencode_decoder(str_vec).unwrap(),
            BencodeType::Bstring(String::from("Test String"))
        );
    }

    #[should_panic]
    #[test]
    fn check_recursive_bencode_decoder_string_len_fails() {
        let str_vec: &Vec<u8> = &String::from("12:Test String").into_bytes();
        assert_eq!(
            recursive_bencode_decoder(str_vec).unwrap(),
            BencodeType::Bstring(String::from("Test String"))
        );
    }
}

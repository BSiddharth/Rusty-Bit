use crate::helper::read_string;
use anyhow::bail;
use std::{
    char,
    collections::BTreeMap,
    fs,
    io::{self, ErrorKind, Write},
    usize,
};

#[allow(dead_code)]
#[derive(PartialEq, Debug)]
enum BencodeType {
    Bstring(String),
    Bvec(Vec<BencodeType>),
    Bmap(BTreeMap<Box<BencodeType>, Box<BencodeType>>),
    Bint(u64),
}
#[allow(dead_code)]
fn recursive_bencode_decoder(data: &Vec<u8>) -> anyhow::Result<(BencodeType, usize)> {
    let mut index: usize = 0;

    // if index is out of range for data at any point, error will be thrown which is expected
    let mut data_char: char = data[index] as char;
    println!("Char is {data_char}");
    if data_char.is_ascii_digit() {
        println!("Entering digit mode");
        let mut len_string: String = String::new();
        while data_char != ':' {
            len_string.push(data_char);
            index += 1;
            data_char = data[index] as char;
        }

        let result_string =
            String::from_utf8(data[index + 1..index + 1 + len_string.parse::<usize>()?].to_vec())?;
        let consumed = result_string.len() + len_string.len() + 1; // +1 for ':'
        println!("returning {result_string}, {consumed}");
        return Ok((BencodeType::Bstring(result_string), consumed));
    } else if data_char == 'i' {
        index += 1;
        data_char = data[index] as char;
        let mut buffer: String = String::new();
        while data_char != 'e' {
            buffer.push(data_char);
            index += 1;
            data_char = data[index] as char;
        }

        let result_int = BencodeType::Bint(buffer.parse::<u64>()?);
        let consumed = buffer.len() + 2; // +2 for 'i' and 'e'

        return Ok((result_int, consumed));
    } else if data_char == 'l' {
        println!("Entering l mode");
        let mut ben_vec: Vec<BencodeType> = Vec::new();
        while data[index] as char != 'e' {
            let data_to_send_start_index: usize = if ben_vec.len() == 0 { 1 } else { 0 };
            let (list_element, new_index) =
                recursive_bencode_decoder(&data[index + data_to_send_start_index..].to_vec())?;
            index += if ben_vec.len() == 0 {
                new_index + 1
            } else {
                new_index
            };
            ben_vec.push(list_element);
        }

        println!("{ben_vec:?}");

        let consumed = ben_vec.len() + 2; // +2 for 'i' and 'e'
        let result_vec = BencodeType::Bvec(ben_vec);

        return Ok((result_vec, consumed));
    }

    println!("Could not parse the bencoded data properly!");
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
            recursive_bencode_decoder(str_vec).unwrap().0,
            BencodeType::Bstring(String::from("Test String"))
        );

        let int_vec: &Vec<u8> = &String::from("i982e").into_bytes();
        assert_eq!(
            recursive_bencode_decoder(int_vec).unwrap().0,
            BencodeType::Bint(982)
        );

        let vec_vec: &Vec<u8> = &String::from("l4:spam4:eggse").into_bytes();
        assert_eq!(
            recursive_bencode_decoder(vec_vec).unwrap().0,
            BencodeType::Bvec(vec![
                BencodeType::Bstring(String::from("spam")),
                BencodeType::Bstring(String::from("eggs"))
            ])
        );
    }

    #[should_panic]
    #[test]
    fn check_recursive_bencode_decoder_string_len_fails() {
        let str_vec: &Vec<u8> = &String::from("12:Test String").into_bytes();
        assert_eq!(
            recursive_bencode_decoder(str_vec).unwrap().0,
            BencodeType::Bstring(String::from("Test String"))
        );
    }

    #[should_panic]
    #[test]
    fn check_recursive_bencode_decoder_int_no_end_fails() {
        let int_vec: &Vec<u8> = &String::from("i32").into_bytes();
        assert_eq!(
            recursive_bencode_decoder(int_vec).unwrap().0,
            BencodeType::Bstring(String::from("Should Panic"))
        );
    }

    #[should_panic]
    #[test]
    fn check_recursive_bencode_decoder_deformed_fails() {
        let data_vec: &Vec<u8> = &String::from("qi23e").into_bytes();
        assert_eq!(
            recursive_bencode_decoder(data_vec).unwrap().0,
            BencodeType::Bstring(String::from("Should Panic"))
        );
    }

    #[should_panic]
    #[test]
    fn check_recursive_bencode_decoder_vec_no_end_fails() {
        let vec_vec: &Vec<u8> = &String::from("l4:spam4:eggs").into_bytes();
        assert_eq!(
            recursive_bencode_decoder(vec_vec).unwrap().0,
            BencodeType::Bstring(String::from("Should Panic"))
        );
    }

    #[should_panic]
    #[test]
    fn check_recursive_bencode_decoder_vec_bad_str_fails() {
        let vec_vec: &Vec<u8> = &String::from("l4:spam3:eggs").into_bytes(); // eggs len should be
                                                                             // 4
        assert_eq!(
            recursive_bencode_decoder(vec_vec).unwrap().0,
            BencodeType::Bstring(String::from("Should Panic"))
        );
    }
}

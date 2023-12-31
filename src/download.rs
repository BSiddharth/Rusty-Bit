use crate::helper::read_string;
use anyhow::{anyhow, bail};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, ErrorKind, Write},
};

#[derive(PartialEq, Debug, PartialOrd, Ord, Eq)]
enum BencodeType {
    Bstring(String),
    Bvec(Vec<BencodeType>),
    Bmap(BTreeMap<Box<BencodeType>, BencodeType>), // needs to be BTreeMap instead of HashMap
    // because HashMap cannot be hashed consistently as it has no fixed order unlike BTreeMap.
    Bint(u64),
}
//
// #[allow(dead_code)]
// enum MapVal {
//     Mstr(String),
//     Mvec(Vec<>),
//     Mmap(HashMap<String, MapVal>),
//     Mint(u64),
// }
//
// #[allow(dead_code)]
// impl BencodeType {
//     fn convert(self) -> MapVal {
//         match self {
//             BencodeType::Bmap(map) => {
//                 let result: HashMap<String, MapVal> = HashMap::new();
//                 for keys in map.keys() {}
//                 MapVal::Mmap(result)
//             }
//             BencodeType::Bstring(s) => MapVal::Mstr(s),
//             BencodeType::Bvec(_) => todo!(),
//             BencodeType::Bint(_) => todo!(),
//         }
//     }
// }

/*
 * This function calls itsellf recursively until the Vec<u8> containing bencoded bytes is resolved
 * (or throws error)
*/
fn recursive_bencode_decoder(data: &Vec<u8>) -> anyhow::Result<(BencodeType, usize)> {
    let mut index: usize = 0;

    // if index is out of range for data at any point, error will be thrown which is expected
    let mut data_char: char = data[index] as char;
    if data_char.is_ascii_digit() {
        let mut string_len_in_string_format: String = String::new();

        while data_char != ':' {
            string_len_in_string_format.push(data_char);
            index += 1;
            data_char = data[index] as char;
        }

        let string_len = string_len_in_string_format.parse::<usize>()?;
        let str_start_index = index + 1;
        let str_end_index = index + 1 + string_len;
        let result_string =
            String::from_utf8_lossy(&data[str_start_index..str_end_index].to_vec()).to_string();
        let consumed = string_len + string_len_in_string_format.len() + 1; // +1 for ':'

        return Ok((BencodeType::Bstring(result_string), consumed));
    } else if data_char == 'i' {
        let start_index = index;
        index += 1;
        data_char = data[index] as char;
        let mut buffer: String = String::new();

        while data_char != 'e' {
            buffer.push(data_char);
            index += 1;
            data_char = data[index] as char;
        }

        let result_int = BencodeType::Bint(buffer.parse::<u64>()?);
        let consumed = index - start_index + 1;

        return Ok((result_int, consumed));
    } else if data_char == 'l' {
        let mut ben_vec: Vec<BencodeType> = Vec::new();
        let start_index = 0;

        while data[index] as char != 'e' {
            let data_to_parse_start_index_offset: usize = if ben_vec.len() == 0 { 1 } else { 0 };
            // when ben_vec is empty it means we are on 'l' and need to send data from the next index,
            // but if it is not empty then it means we do not need to skip ahead 1 index as the
            // vec elements are concatinated without any delimiter.

            let (list_element, len_consumed) = recursive_bencode_decoder(
                &data[index + data_to_parse_start_index_offset..].to_vec(),
            )?;
            index += if ben_vec.len() == 0 {
                len_consumed + 1
            } else {
                len_consumed
            };
            ben_vec.push(list_element);
        }

        let consumed = index - start_index + 1;
        let result_vec = BencodeType::Bvec(ben_vec);

        return Ok((result_vec, consumed));
    } else if data_char == 'd' {
        let mut ben_map: BTreeMap<Box<BencodeType>, BencodeType> = BTreeMap::new();
        let mut send_data_from_next_index = true;
        let start_index = index;

        while data[index] as char != 'e' {
            let (key, new_index) = extract_btype(&data, index, send_data_from_next_index)?;
            index = new_index;
            send_data_from_next_index = false;

            let (value, new_index) = extract_btype(&data, index, send_data_from_next_index)?;
            index = new_index;

            ben_map.insert(Box::new(key), value);
        }

        let consumed = index - start_index + 1;
        let result_vec = BencodeType::Bmap(ben_map);

        return Ok((result_vec, consumed));
    }

    println!("Could not parse the bencoded data properly!");
    bail!("Could not parse the bencoded data properly!")
}

/*
 * Helper for the dictionary part of recursive_bencode_decoder
*/
fn extract_btype(
    data: &[u8],
    index: usize,
    send_data_from_next_index: bool,
) -> anyhow::Result<(BencodeType, usize)> {
    let data_to_send_start_index: usize = if send_data_from_next_index { 1 } else { 0 };
    let (data, new_index) =
        recursive_bencode_decoder(&data[index + data_to_send_start_index..].to_vec())?;
    let return_index = index
        + if send_data_from_next_index {
            new_index + 1
        } else {
            new_index
        };
    Ok((data, return_index))
}

/*
 * This function is responsible for converting the data in bencoded file into rust datatype.
*/
fn decode_bencoded_file(file_path: String) -> anyhow::Result<BencodeType> {
    println!("Trying to read file {file_path}");
    let file_data = fs::read(&file_path);
    match file_data {
        Ok(file_data_vec) => {
            println!("Decoding bencoded file {file_path}");
            let decoder_result = recursive_bencode_decoder(&file_data_vec);
            match decoder_result {
                Ok((decoded_result, _)) => {
                    println!("File decoded succesfully!");
                    Ok(decoded_result)
                }
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

    match decoded_file_data {
        BencodeType::Bmap(map) => {
            let announce = map
                .get(&BencodeType::Bstring(String::from("announce")))
                .ok_or(anyhow!("announce does not exist in the map"))?;
            println!("Starting download now, trying to contact {:?}", announce);
        }
        _ => bail!("Bencoded file format wrong -> not a map"),
    }
    Ok(())
}

#[cfg(test)]
mod download_test {
    use std::collections::BTreeMap;

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

        let map_vec: &Vec<u8> = &String::from("d4:spaml1:a1:bee").into_bytes();
        let btmap = BTreeMap::from([(
            Box::new(BencodeType::Bstring(String::from("spam"))),
            BencodeType::Bvec(vec![
                BencodeType::Bstring(String::from("a")),
                BencodeType::Bstring(String::from("b")),
            ]),
        )]);
        assert_eq!(
            recursive_bencode_decoder(map_vec).unwrap().0,
            BencodeType::Bmap(btmap)
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
    #[should_panic]
    #[test]
    fn check_recursive_bencode_decoder_map_bad_str_fails() {
        let map_vec: &Vec<u8> = &String::from("d4:spaml1:a1:bbee").into_bytes();
        assert_eq!(
            recursive_bencode_decoder(map_vec).unwrap().0,
            BencodeType::Bstring(String::from("Should Panic"))
        );
    }
    #[should_panic]
    #[test]
    fn check_recursive_bencode_decoder_bad_map_fails() {
        let map_vec: &Vec<u8> = &String::from("d4:spaml1:a1:be").into_bytes();
        assert_eq!(
            recursive_bencode_decoder(map_vec).unwrap().0,
            BencodeType::Bstring(String::from("Should Panic"))
        );
    }
}

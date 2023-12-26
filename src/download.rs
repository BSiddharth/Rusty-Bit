use std::{
    fs,
    io::{self, ErrorKind, Write},
};

use crate::helper::read_string;

fn decode_bencoded_file(file_path: String) -> anyhow::Result<()> {
    println!("Trying to read file {file_path}");
    let file_data = fs::read(&file_path);
    match file_data {
        Ok(file_data_vec) => {
            println!("Decoding bencoded file {file_path}");
            print!("{}", String::from_utf8_lossy(&file_data_vec));
            Ok(())
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
    use crate::download::decode_bencoded_file;

    #[test]
    fn check_decode_becoded_file_fails() {
        let file_path: String = "non exisitng file path".to_string();
        assert!(decode_bencoded_file(file_path).is_err());
    }

    #[test]
    fn check_decode_becoded_file_passes() {
        let file_path: String = r"C:\Users\SIDDHARTH\Desktop\sample.torrent".to_string();
        assert!(decode_bencoded_file(file_path).is_ok());
    }
}

use std::{
    fs,
    io::{self, ErrorKind, Write},
};
fn read_string() -> String {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("Could not read the input!!");
    input
}
fn decode_bencoded_file(file_path: String) -> anyhow::Result<()> {
    println!("Reading file at {file_path}");
    let file_data = fs::read(&file_path);
    match file_data {
        Ok(_) => {
            println!("Will try to decode this");
            Ok(())
        }
        Err(e) => {
            println!("{e:?}");
            if e.kind() == ErrorKind::NotFound {
                println!("File path does not exist, check the file path again!!");
            } else {
                println!("Could not read the file!!");
            }
            Err(e.into())
        }
    }
}

fn download_using_file(file_path: String) -> anyhow::Result<()> {
    decode_bencoded_file(file_path)?;
    Ok(())
}
fn main() {
    println!(
        r"
______          _          ______ _ _   
| ___ \        | |         | ___ (_) |  
| |_/ /   _ ___| |_ _   _  | |_/ /_| |_ 
|    / | | / __| __| | | | | ___ \ | __|
| |\ \ |_| \__ \ |_| |_| | | |_/ / | |_ 
\_| \_\__,_|___/\__|\__, | \____/|_|\__|
                     __/ |              
                    |___/               
"
    );

    loop {
        println!("What would you like to do:\n    1) Download using .torrent file\n    2) Download using magnet link\n    3) Quit Rusty-Bit\n");
        print!("Choose your preferred download method or quit the program: ");
        io::stdout().flush().expect("Couldn't flush stdout");
        let chosen_option = read_string();
        match chosen_option.trim() {
            "1" => {
                print!("You chose to download using .torrent file, provide the file path: ");
                io::stdout().flush().expect("Couldn't flush stdout");
                let file_path = read_string();
                let result = download_using_file(file_path.trim().to_owned());
                if result.is_ok() {
                    break;
                }
            }
            "2" => {
                println!("You chose to download using magnet link");
                break;
            }
            "3" => {
                println!("See you later");
                break;
            }
            _ => println!("Option should be a number from the table given above!! Try again\n"),
        }
    }
}

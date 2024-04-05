use rusty_bit::{
    download::download_using_file,
    helper::{self, print_single_ln},
};
use std::io::{self, Write};

#[tokio::main]
async fn main() {
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

    // TODO : Use clap for command line parsing
    loop {
        println!(
            "\nWhat would you like to do:\n\
        1) Download using .torrent file\n\
        2) Download using magnet link\n\
        3) Quit Rusty-Bit\n"
        );
        print_single_ln("Choose your preferred download method or quit the program: ");
        let chosen_option = helper::read_string();
        match chosen_option.as_str() {
            "1" => {
                let download_result = download_using_file().await;
                if download_result.is_ok() {
                    println!("Download completed, exiting...");
                    println!("See you later");
                } else {
                    println!("Download failed, reason: {:?}", download_result.err());
                }
                break;
            }
            "2" => {
                println!("You chose to download using magnet link");
                break;
            }
            "3" => {
                println!("See you later");
                break;
            }
            _ => println!("Option should be a number from the table given above!! Try again.\n"),
        }
    }
}

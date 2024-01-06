use rusty_bit::{download::download_using_file, helper};
use std::io::{self, Write};

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

    // TODO : Use clap for command line parsing
    loop {
        println!(
            "\nWhat would you like to do:\n\
        1) Download using .torrent file\n\
        2) Download using magnet link\n\
        3) Quit Rusty-Bit\n"
        );
        print!("Choose your preferred download method or quit the program: ");
        io::stdout().flush().expect("Couldn't flush stdout");
        let chosen_option = helper::read_string();
        match chosen_option.as_str() {
            "1" => {
                let result = download_using_file();
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
            _ => println!("Option should be a number from the table given above!! Try again.\n"),
        }
    }
}

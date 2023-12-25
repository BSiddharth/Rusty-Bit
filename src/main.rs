use std::io::{self, Write};

fn read_string() -> String {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("Could not read the input!!");
    input
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

    println!("What would you like to do:\n    1) Download using .torrent file\n    2) Download using magnet link\n    3) Quit Rusty-Bit\n");
    loop {
        print!("Choose your preffered download method or quit the program: ");
        io::stdout().flush().expect("Couldn't flush stdout");
        let chosen_option = read_string();
        match chosen_option.trim() {
            "1" => {
                println!("You chose to download using .torrent file");
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
            _ => println!("Option should be a number from the table given above!! Try again\n"),
        }
    }
}

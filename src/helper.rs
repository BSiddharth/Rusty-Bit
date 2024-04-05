use std::io::{self, Write};

pub fn read_string() -> String {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("Could not read the input!!");
    input.trim().to_owned()
}

pub fn print_single_ln(input: &str) {
    print!("{input}");
    io::stdout().flush().expect("Couldn't flush stdout");
}

pub fn read_string() -> String {
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("Could not read the input!!");
    input.trim().to_owned()
}

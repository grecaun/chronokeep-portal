use std::io;

pub mod database;
pub mod network;
pub mod objects;
pub mod reader;

//const ZERO_CONF_PORT: u32 = 12345;
//const CONTROL_PORT: u32 = 12346;

fn main() {
    println!("Chronokeep Portal starting up...");
    let mut keepalive: bool = true;
    let mut input: String = String::new();

    while keepalive {
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line.");
        let lowercase: String = input.to_lowercase();
        let parts: Vec<&str> = lowercase.split_whitespace().collect();
        let first: &str = if parts.len() > 0 {parts[0]} else {""};
        input.clear();
        match first {
            "q" | "quit" => {
                keepalive = false;
                println!("Quit command given.")
            },
            "h" => print_help(),
            option => println!("'{option}' is not a valid command. Type h for help.")
        };
    }
    println!("Goodbye!")
}

fn print_help() {
    // TODO - Add help section for commands
    println!("Help section goes here.")
}
use core::panic;
use std::io;

use crate::database::sqlite;
use crate::database::Database;

pub mod control;
pub mod defaults;
pub mod database;
pub mod network;
pub mod objects;
pub mod reader;
pub mod util;

fn main() {
    println!("Chronokeep Portal starting up...");
    let sqlite = sqlite::SQLite::new().unwrap();
    match sqlite.setup() {
        Ok(_) => println!("Database successfully setup."),
        Err(e) => {
            println!("Error setting up database: {e}");
            panic!()
        }
    }
    let control = control::Control::new(&sqlite).unwrap();
    println!("Control values retrieved from database.");
    let s: u64 = u64::from(control.sighting_period);
    println!("Sightings will be ignored if received within {}", util::pretty_time(&s));
    println!("Zero Conf Port: {} -- Control Port: {}", control.zero_conf_port, control.control_port);
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
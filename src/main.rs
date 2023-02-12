use core::panic;
use std::env;
use std::sync::Arc;
use std::sync::Mutex;

use crate::database::sqlite;
use crate::database::Database;

pub mod control;
pub mod defaults;
pub mod database;
pub mod network;
pub mod objects;
pub mod reader;
pub mod types;
pub mod util;
pub mod llrp;

const CONTROL_TYPE: &str = "socket";

fn main() {
    println!("Chronokeep Portal starting up...");
    let mut sqlite = sqlite::SQLite::new().unwrap();
    match sqlite.setup() {
        Ok(_) => println!("Database successfully setup."),
        Err(e) => {
            println!("Error setting up database: {e}");
            panic!()
        }
    }
    let control = control::Control::new(&sqlite).unwrap();
    let sqlite = Arc::new(Mutex::new(sqlite));
    println!("Control values retrieved from database.");
    println!("Portal is named '{}'.", control.name);
    println!("Sightings will be ignored if received within {}", util::pretty_time(&u64::from(control.sighting_period)));
    let args: Vec<String> = env::args().collect();
    if args.len() > 0 && args[0].as_str() == "daemon" {
        control::socket::control_loop(sqlite.clone(), control)
    }  else {
        match CONTROL_TYPE {
            "socket" => {
                control::socket::control_loop(sqlite.clone(), control)
            },
            "cli" => {
                control::cli::control_loop(sqlite.clone(), control);
            },
            other => {
                println!("'{other}' is not a valid control type.");
            }
        }
    }
    println!("Goodbye!")
}
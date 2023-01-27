use core::panic;
use std::env;

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
    println!("Portal is named '{}'.", control.name);
    println!("Sightings will be ignored if received within {}", util::pretty_time(&u64::from(control.sighting_period)));
    println!("Zero Conf Port: {} -- Control Port: {}", control.zero_conf_port, control.control_port);
    let args: Vec<String> = env::args().collect();
    if args.len() > 0 && args[0].as_str() == "daemon" {
        // implement server control logic
        // control::server::control_loop(&sqlite);
    }  else {
        control::cli::control_loop(&sqlite);
    }
    println!("Goodbye!")
}
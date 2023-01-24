use core::panic;
use std::io;
use std::str::FromStr;

use crate::database::sqlite;
use crate::database::Database;
use crate::objects::setting;
use crate::reader::zebra;

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
    println!("Portal is named '{}'.", control.name);
    println!("Sightings will be ignored if received within {}", util::pretty_time(&u64::from(control.sighting_period)));
    println!("Zero Conf Port: {} -- Control Port: {}", control.zero_conf_port, control.control_port);
    let mut keepalive: bool = true;
    let mut input: String = String::new();

    while keepalive {
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line.");
        let in_string = input.to_string();
        let parts: Vec<&str> = in_string.split_whitespace().collect();
        let first_str = if parts.len() > 0 {parts[0].to_lowercase()} else {String::from("")};
        let first: &str = first_str.as_str();
        input.clear();
        match first {
            "r" | "reader" => {
                match parts[1].to_lowercase().as_str() {
                    "a" | "add" => {
                        if parts.len() < 5 {
                            println!("Invalid number of arguments specified.");
                            continue
                        }
                        let port = if parts.len() < 6  {""} else {parts[5]};
                        add_reader(parts[2], parts[3].to_lowercase().as_str(), parts[4], port, &sqlite);
                    },
                    "c" | "connect" => {
                        println!("not yet implemented");
                    },
                    "l" | "list" => {
                        list_readers(&sqlite);
                    }
                    other => {
                        println!("'{other}' is not a valid option for readers.");
                    }
                }
            }
            "s" | "setting" => {
                if parts.len() < 3 {
                    println!("Invalid number of arguments specified.");
                    continue
                }
                change_setting(parts[1].to_lowercase().as_str(), parts[2], &sqlite);
            },
            "q" | "quit" | "exit" => {
                keepalive = false;
            },
            "h" | "help" => print_help(),
            option => println!("'{option}' is not a valid command. Type h for help."),
        };
    }
    println!("Goodbye!")
}

fn list_readers(sqlite: &sqlite::SQLite) {
    let res = sqlite.get_readers();
    match res {
        Ok(readers) => {
            if readers.len() == 0 {
                println!("No readers saved.");
                return
            }
            for reader in readers {
                println!("Reader      - {}", reader.nickname());
                println!("      Kind  - {}", reader.kind());
                println!("      IP    - {}", reader.ip_address());
                println!("      Port  - {}", reader.port());
            }
        },
        Err(e) => {
            println!("Error retrieving readers. {e}");
        }
    }
}

fn add_reader(name: &str, kind: &str, ip: &str, port: &str, sqlite: &sqlite::SQLite) {
    match kind {
        "z" | "zebra" => {
            let port: u16 = u16::from_str(port).unwrap_or_else(|_err| {
                println!("Invalid or no port specified. Using default.");
                zebra::DEFAULT_ZEBRA_PORT
            });
            match sqlite.save_reader(&zebra::Zebra::new(
                0,
                String::from(name),
                String::from(ip),
                port
            )) {
                Ok(_) => {
                    println!("Reader saved.")
                },
                Err(e) => {
                    println!("Unable to save reader. {e}")
                }
            }
        },
        kind => {
            println!("'{kind}' is not a valid reader type.")
        }
    }
}

fn change_setting(setting: &str, value: &str, sqlite: &sqlite::SQLite) {
    match setting {
        "s" | "sightings" => {
            let p: Vec<&str> = value.split(':').collect();
            match p.len() {
                2 => {
                    if let Ok(minutes) = u64::from_str(p[0]) {
                        if let Ok(seconds) = u64::from_str(p[1]) {
                            let val = (minutes * 60) + seconds;
                            let res = sqlite.set_setting(&setting::Setting::new(String::from(control::SETTING_SIGHTING_PERIOD), val.to_string()));
                            match res {
                                Ok(_) => println!("Sighting period set to {}.", util::pretty_time(&val)),
                                Err(e) => println!("Unable to set sighting period. {e}"),
                            }
                            return
                        }
                    }
                },
                3 => {
                    if let Ok(hours) = u64::from_str(p[0]) {
                        if let Ok(minutes) = u64::from_str(p[1]) {
                            if let Ok(seconds) = u64::from_str(p[1]) {
                                let val = (hours * 3600) + (minutes * 60) + seconds;
                                let res = sqlite.set_setting(&setting::Setting::new(String::from(control::SETTING_SIGHTING_PERIOD), val.to_string()));
                                match res {
                                    Ok(_) => println!("Sighting period set to {}.", util::pretty_time(&val)),
                                    Err(e) => println!("Unable to set sighting period. {e}"),
                                }
                                return
                            }
                        }
                    }
                },
                1 => {
                    if let Ok(seconds) = u64::from_str(value) {
                        let res = sqlite.set_setting(&setting::Setting::new(String::from(control::SETTING_SIGHTING_PERIOD), seconds.to_string()));
                        match res {
                            Ok(_) => println!("Sighting period set to {}.", util::pretty_time(&seconds)),
                            Err(e) => println!("Unable to set sighting period. {e}"),
                        }
                        return;
                    }
                },
                _ => {}
            }
            println!("Invalid time value for sighting period specified. Type h for help.");
        },
        "z" | "zeroconf" => {
            if let Ok(port) = u16::from_str(value) {
                let res = sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_ZERO_CONF_PORT),
                    port.to_string()));
                match res {
                    Ok(_) => println!("Zero configuration port set to {}.", port),
                    Err(e) => println!("Unable to set zero configuration port. {e}"),
                }
                return;
            }
            println!("Invalid port specified. Type h for help.")
        },
        "c" | "control" => {
            if let Ok(port) = u16::from_str(value) {
                let res = sqlite.set_setting(&setting::Setting::new(
                    String::from(control::SETTING_CONTROL_PORT),
                    port.to_string()));
                match res {
                    Ok(_) => println!("Control port set to {}.", port),
                    Err(e) => println!("Unable to set control port. {e}"),
                }
                return;
            }
            println!("Invalid port specified. Type h for help.")
        },
        "n" | "name" => {
            let res = sqlite.set_setting(&setting::Setting::new(
                String::from(control::SETTING_PORTAL_NAME),
                String::from(value),
            ));
            match res {
                Ok(_) => println!("Portal name set to '{}'.", value),
                Err(e) => println!("Unable to set portal name. {e}"),
            }
            return;
        },
        option => {
            println!("'{option} is not a valid option for a setting. Type h for help.");
        }
    }
}

fn print_help() {
    println!("(s)etting -- Type s or setting to change a setting.  Valid values to change are:");
    println!("    (s)ighting <X> - Define the period of time where we should ignore any subsequent chip reads after the first. Can be given in number of seconds or (h):MM:ss format.");
    println!("    (z)eroconf <X> - Define the port to be used for the zero configuration lookup utility. Useful for determining the IP of this machine. 1-65356");
    println!("    (c)ontrol  <X> - Define the port to be used for connecting to the control and information command interfaces. 1-65356")
}
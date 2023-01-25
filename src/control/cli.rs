
use std::io;
use std::str::FromStr;
use std::thread::JoinHandle;

use crate::database::Database;
use crate::database::sqlite;
use crate::objects::setting;
use crate::reader::{self, Reader};
use crate::reader::zebra;
use crate::util;

pub fn control_loop(sqlite: &sqlite::SQLite) {
    let mut keepalive: bool = true;
    let mut input: String = String::new();
    let mut connected: Vec<Box<dyn reader::Reader>> = Vec::new();
    let mut joiners: Vec<JoinHandle<()>> = Vec::new();

    while keepalive {
        // read standard in
        io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line.");
        // copy input string, split on whitespace and collect for parsing
        let in_string = input.to_string();
        let parts: Vec<&str> = in_string.split_whitespace().collect();
        // check if anything was input and make lowercase if so
        let first = if parts.len() > 0 {parts[0].to_lowercase()} else {String::from("")};
        input.clear();
        match first.as_str() {
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
                        if parts.len() > 5 {
                            let port = if parts.len() < 6  {""} else {parts[5]};
                            add_reader(parts[2], parts[3].to_lowercase().as_str(), parts[4], port, &sqlite);
                        }
                        if parts.len() < 3 {
                            println!("Invalid number of arguments specified.");
                            continue
                        }
                        let reader = match sqlite.get_reader(parts[2]) {
                            Ok(r) => r,
                            Err(e) => {
                                println!("Unable to connect to the reader. {e}");
                                continue
                            },
                        };
                        match reader.kind() {
                            reader::READER_KIND_ZEBRA => {
                                let mut r = reader::zebra::Zebra::new(
                                    reader.id(),
                                    String::from(reader.nickname()),
                                    String::from(reader.ip_address()),
                                    reader.port(),
                                );
                                match r.connect() {
                                    Ok(j) => {
                                        connected.push(Box::new(r));
                                        joiners.push(j)
                                    },
                                    Err(e) => println!("Error connecting to reader. {e}"),
                                }
                            },
                            _ => {
                                println!("unknown reader type found")
                            }
                        }
                    },
                    "d" | "disconnect" => {
                        if parts.len() < 3 {
                            println!("Invalid number of arguments specified.");
                            continue
                        }
                        let mut found = false;
                        let index = match connected.iter().position(|x| x.nickname() == parts[2]) {
                            Some(ix) => {
                                found = true;
                                ix
                            },
                            None => {
                                0
                            },
                        };
                        if found {
                            let mut reader = connected.remove(index);
                            match reader.disconnect() {
                                Ok(_) => println!("Successfully disconnected from {}.", reader.nickname()),
                                Err(e) => println!("Error disconnecting from the reader. {e}"),
                            }
                        }                     
                    }
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
            "" => (),
            option => println!("'{option}' is not a valid command. Type h for help."),
        };
    }

    for reader in connected.iter_mut() {
        match reader.disconnect() {
            Ok(_) => println!("Disconnected from {}", reader.nickname()),
            Err(e) => println!("Error disconnecting from {}. {e}", reader.nickname()),
        }
    }
    while joiners.len() > 0 {
        let cur_thread = joiners.remove(0);
        match cur_thread.join() {
            Ok(_) => (),
            Err(e) => println!("Join failed. {:?}", e),
        }
    }
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
                println!("Reader {0:<4} - {1}", reader.id(), reader.nickname());
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
                            let res = sqlite.set_setting(&setting::Setting::new(String::from(super::SETTING_SIGHTING_PERIOD), val.to_string()));
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
                                let res = sqlite.set_setting(&setting::Setting::new(String::from(super::SETTING_SIGHTING_PERIOD), val.to_string()));
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
                        let res = sqlite.set_setting(&setting::Setting::new(String::from(super::SETTING_SIGHTING_PERIOD), seconds.to_string()));
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
                    String::from(super::SETTING_ZERO_CONF_PORT),
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
                    String::from(super::SETTING_CONTROL_PORT),
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
                String::from(super::SETTING_PORTAL_NAME),
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
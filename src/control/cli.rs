
use std::io;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::JoinHandle;

use crate::database::Database;
use crate::database::sqlite;
use crate::network::api;
use crate::objects::setting;
use crate::reader::{self, Reader};
use crate::reader::zebra;
use crate::util;

pub fn control_loop(sqlite: Arc<Mutex<sqlite::SQLite>>, controls: super::Control) {
    let mut keepalive: bool = true;
    let mut input: String = String::new();
    let mut connected: Vec<reader::Reader> = Vec::new();
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
                        let sq = match sqlite.lock() {
                            Ok(sq) => sq,
                            Err(_) => {
                                println!("Error grabbing database mutex.");
                                continue
                            }
                        };
                        add_reader(parts[2], parts[3].to_lowercase().as_str(), parts[4], port, &sq);
                    },
                    "c" | "connect" => {
                        if parts.len() > 5 {
                            let port = if parts.len() < 6  {""} else {parts[5]};
                            let sq = match sqlite.lock() {
                                Ok(sq) => sq,
                                Err(_) => {
                                    println!("Error grabbing database mutex.");
                                    continue
                                }
                            };
                            let id = add_reader(parts[2], parts[3].to_lowercase().as_str(), parts[4], port, &sq);
                            drop(sq);
                            if id > 0 {
                                connect_reader(id, &sqlite, &mut connected, &mut joiners, &controls);
                            }
                            continue
                        }
                        if parts.len() < 3 {
                            println!("Invalid number of arguments specified.");
                            continue
                        }
                        if let Ok(id) = i64::from_str(parts[2]) {
                            connect_reader(id, &sqlite, &mut connected, &mut joiners, &controls);
                        } else {
                            println!("Invalid reader number.");
                        }
                    },
                    "d" | "disconnect" => {
                        if parts.len() < 3 {
                            println!("Invalid number of arguments specified.");
                            continue
                        }
                        let id: i64 = match i64::from_str(parts[2]) {
                            Ok(v) => v,
                            Err(_) => {
                                println!("Invalid reader number specified.");
                                continue
                            },
                        };
                        disconnect_reader(id, &mut connected);
                    },
                    "r" | "remove" => {
                        if parts.len() < 3 {
                            println!("Invalid number of arguments specified.");
                            continue
                        }
                        let id: i64 = match i64::from_str(parts[2]) {
                            Ok(v) => v,
                            Err(_) => {
                                println!("Invalid reader number specified.");
                                continue
                            },
                        };
                        let sq = match sqlite.lock() {
                            Ok(sq) => sq,
                            Err(_) => {
                                println!("Error grabbing database mutex.");
                                continue
                            }
                        };
                        remove_reader(id, &sq, &mut connected);
                    },
                    "l" | "list" => {
                        let sq = match sqlite.lock() {
                            Ok(sq) => sq,
                            Err(_) => {
                                println!("Error grabbing database mutex.");
                                continue
                            }
                        };
                        list_readers(&sq);
                    },
                    "s" | "send" => {
                        if parts.len() < 3 {
                            println!("Invalid number of arguments specified.");
                            continue
                        }
                        let id: i64 = match i64::from_str(parts[2]) {
                            Ok(v) => v,
                            Err(_) => {
                                println!("Invalid reader number specified.");
                                continue
                            },
                        };
                        match connected.iter().position(|x| x.id() == id) {
                            Some(ix) => {
                                let mut reader =  connected.remove(ix);
                                match reader.initialize() {
                                    Ok(_) => (),
                                    Err(e) => println!("Error initializing reader. {e}")
                                };
                                connected.push(reader);
                            },
                            None => {
                                println!("Unable to find reader.")
                            }
                        }
                    },
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
                let sq = match sqlite.lock() {
                    Ok(sq) => sq,
                    Err(_) => {
                        println!("Error grabbing database mutex.");
                        continue
                    }
                };
                change_setting(parts[1].to_lowercase().as_str(), parts[2], &sq);
            },
            "a" | "api" => {
                match parts[1].to_lowercase().as_str() {
                    "a" | "add" => {
                        // name, type, token, uri
                        if parts.len() < 5 {
                            println!("Invalid number of arugments specified.");
                            continue
                        }
                        let uri = if parts.len() < 6 {"https://api.chronokeep.com/"} else {parts[5]};
                        let sq = match sqlite.lock() {
                            Ok(sq) => sq,
                            Err(_) => {
                                println!("Error grabbing database mutex.");
                                continue
                            }
                        };
                        add_api(parts[2], parts[3], parts[4], uri, &sq);
                    },
                    "l" | "list" => {
                        let sq = match sqlite.lock() {
                            Ok(sq) => sq,
                            Err(_) => {
                                println!("Error grabbing database mutex.");
                                continue
                            }
                        };
                        list_api(&sq);
                    },
                    "r" | "remove" => {
                        if parts.len() < 3 {
                            println!("Invalid number of arguments specified.");
                            continue
                        }
                        let sq = match sqlite.lock() {
                            Ok(sq) => sq,
                            Err(_) => {
                                println!("Error grabbing database mutex.");
                                continue
                            }
                        };
                        remove_api(parts[2], &sq);
                    },
                    _ => {
                        println!("Unknown command.");
                        continue
                    },
                }
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

fn disconnect_reader(id: i64, connected: &mut Vec<Reader>){
    match connected.iter().position(|x| x.id() == id) {
        Some(ix) => {
            let mut reader = connected.remove(ix);
            match reader.disconnect() {
                Ok(_) => println!("Successfully disconnected from {}.", reader.nickname()),
                Err(e) => println!("Error disconnecting from the reader. {e}"),
            }
        },
        None => {
            println!("Reader not found.")
        },
    };
}

fn remove_reader(
    id: i64,
    sqlite: &sqlite::SQLite,
    connected: &mut Vec<reader::Reader>,
) {
    match connected.iter().position(|x| x.id() == id) {
        Some(ix) => {
            let mut reader = connected.remove(ix);
            match reader.disconnect() {
                Ok(_) => println!("Successfully disconnected from {}.", reader.nickname()),
                Err(e) => println!("Error disconnecting from the reader. {e}"),
            }
        },
        None => (),
    }
    match sqlite.delete_reader(&id) {
        Ok(_) => println!("Successfully removed Reader {id} from saved reader list."),
        Err(e) => println!("Error removing Reader {id} from saved reader list. {e}"),
    }
}

fn connect_reader(
    id: i64,
    mtx: &Arc<Mutex<sqlite::SQLite>>,
    connected: &mut Vec<reader::Reader>,
    joiners: &mut Vec<JoinHandle<()>>,
    controls: &super::Control
) {
    let sqlite = match mtx.lock() {
        Ok(v) => v,
        Err(e) => {
            println!("Unable to get database mutex. {e}");
            return
        }
    };
    let reader = match sqlite.get_reader(&id) {
        Ok(r) => r,
        Err(e) => {
            println!("Unable to connect to the reader. {e}");
            return
        },
    };
    drop(sqlite);
    match reader.kind() {
        reader::READER_KIND_ZEBRA => {
            let mut r = match reader::Reader::new_no_repeaters(
                reader.id(),
                String::from(reader.kind()),
                String::from(reader.nickname()),
                String::from(reader.ip_address()),
                reader.port(),
                reader.auto_connect(),
            ) {
                Ok(it) => it,
                Err(e) => {
                    println!("error creating reader {e}");
                    return;
                }
            };
            match r.connect(mtx, &controls) {
                Ok(j) => {
                    connected.push(r);
                    joiners.push(j);
                },
                Err(e) => {
                    println!("Error connecting to reader. {e}");
                },
            }
        },
        _ => {
            println!("unknown reader type found");
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

fn list_api(sqlite: &sqlite::SQLite) {
    let res = sqlite.get_apis();
    match res {
        Ok(apis) => {
            if apis.len() == 0 {
                println!("No apis saved.");
                return
            }
            for api in apis {
                println!("API {0:<4}  - {1}", api.id(), api.nickname());
                println!("     Kind  - {}", api.kind());
                println!("     URI   - {}", api.uri());
                println!("     Token - {}", api.token());
            }
        },
        Err(e) => {
            println!("Error retrieving readers. {e}");
        }
    }
}

fn add_api(name: &str, kind: &str, token: &str, uri: &str, sqlite: &sqlite::SQLite) {
    let l_kind: &str;
    let l_uri: &str;
    match kind {
        "c" | "chronokeep_results" => {
            l_kind = api::API_TYPE_CHRONOKEEP_RESULTS;
            l_uri = api::API_URI_CHRONOKEEP_RESULTS;
        },
        "chronokeep_results_self" => {
            l_kind = api::API_TYPE_CKEEP_RESULTS_SELF;
            l_uri = uri;
        },
        "chronokeep_remote" => {
            l_kind = api::API_TYPE_CHRONOKEEP_REMOTE;
            l_uri = api::API_URI_CHRONOKEEP_REMOTE;
        },
        "chronokeep_remote_self" => {
            l_kind = api::API_TYPE_CKEEP_REMOTE_SELF;
            l_uri = uri;
        },
        kind => {
            println!("'{kind}' is not a valid api type.");
            return
        },
    }
    match sqlite.save_api(&api::Api::new(
        0,
        String::from(name),
        String::from(l_kind),
        String::from(token),
        String::from(l_uri)
    )) {
        Ok(_) => {
            println!("API saved.");
        },
        Err(e) => {
            println!("Unable to save API. {e}");
        }
    }
}

fn remove_api(name: &str, sqlite: &sqlite::SQLite) {
    match sqlite.delete_api(name) {
        Ok(_) => println!("Successfully removed {name} from saved api list."),
        Err(e) => println!("Error removing the api from saved api list. {e}"),
    }
}

fn add_reader(name: &str, kind: &str, ip: &str, port: &str, sqlite: &sqlite::SQLite) -> i64 {
    match kind {
        "z" | "zebra" => {
            let port: u16 = u16::from_str(port).unwrap_or_else(|_err| {
                zebra::DEFAULT_ZEBRA_PORT
            });
            let tmp = match reader::Reader::new_no_repeaters(
                0,
                String::from(reader::READER_KIND_ZEBRA),
                String::from(name),
                String::from(ip),
                port,
                reader::AUTO_CONNECT_FALSE
            ) {
                Ok(it) => it,
                Err(e) => {
                    println!("error making reader object {e}");
                    return -1;
                }
            };
            match sqlite.save_reader(&tmp) {
                Ok(val) => {
                    println!("Reader saved.");
                    return val
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
    -1
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
    println!("    (s)ighting <X>    - Define the period of time where we should ignore any subsequent chip reads after the first. Can be given in number of seconds or (h):MM:ss format.");
    println!("    (n)ame     <X>    - Changes the advertised name of this device.");
    println!("(r)eader  -- Type r or reading to deal with readers. Valid values are:");
    println!("    (l)ist            - List all saved readers. Number is used for other commands.");
    println!("    (a)dd <name> <kind> <ip> [port] - Save a reader with name, kind, ip, and optional port.");
    println!("                      - Valid kinds are (l)lrp.");
    println!("    (c)onnect <#>     - Connect to a reader.");
    println!("    (d)isconnect <#>  - Disconnect from a reader.");
    println!("    (r)emove <#>      - Remove a reader from the saved readers list.");
    println!("    (s)end <c>        - Send a specified command.");
    println!("(h)elp                - Displays this help message.");
    println!("(q)uit                - Instructs the program to terminate.");
}
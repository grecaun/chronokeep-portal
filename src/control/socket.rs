use std::{thread::{JoinHandle, self}, sync::{Mutex, Arc, MutexGuard}, net::{TcpListener, TcpStream, Shutdown}, io::{Read, ErrorKind, Write}, time::{SystemTime, UNIX_EPOCH, Duration}};

use chrono::Utc;

use crate::{database::{sqlite, Database}, reader::{self, zebra, Reader}, objects::{setting, participant, read}, network::api, control::SETTING_PORTAL_NAME};

use super::zero_conf::ZeroConf;

pub mod requests;
pub mod responses;
pub mod errors;

pub const MAX_CONNECTED: usize = 4;
pub const CONNECTION_TYPE: &str = "chrono_portal";
pub const CONNECTION_VERS: usize = 1;

pub const READ_TIMEOUT_SECONDS: u64 = 5;
pub const KEEPALIVE_INTERVAL_SECONDS: u64 = 30;

pub fn control_loop(sqlite: Arc<Mutex<sqlite::SQLite>>, controls: super::Control) {
    // Keepalive is the boolean that tells us if we need to keep running.
    let keepalive: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));

    // Joiners are join handles for threads we spin up.
    let joiners: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    // Readers are chip readers that are saved.  They may be connected or reading as well.
    let readers: Arc<Mutex<Vec<Box<dyn reader::Reader>>>> = Arc::new(Mutex::new(Vec::new()));

    // Control sockets are sockets that are connected and should be relayed any changes in settings / readers / apis
    // when another socket changes/deletes/adds something. -- Last spot is reserved for localhost to send shutdown command
    let control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>> = Arc::new(Mutex::new(Default::default()));
    // Read repeaters are sockets that want reads to be sent to them as they're being saved.
    let read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>> = Arc::new(Mutex::new([false;MAX_CONNECTED]));
    // Sighting repeaters are sockets that want sightings to be sent to them as they're being saved.
    let sighting_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>> = Arc::new(Mutex::new([false;MAX_CONNECTED]));
    
    // Our control port will be semi-random at the start to try to ensure we don't try to get a port in use.
    let control_port = get_available_port();

    let listener = match TcpListener::bind(("0.0.0.0", control_port)) {
        Ok(list) => list,
        Err(e) => {
            println!("Error opening listener. {e}");
            return
        }
    };

    // create our zero configuration udp socket struct
    let zero = match ZeroConf::new(
        sqlite.clone(),
        &control_port,
        keepalive.clone()
    ) {
        Ok(zc) => zc,
        Err(e) => {
            println!("Error getting zero conf: {e}");
            return
        }
    };
    // then start the thread and push the join handle to our bunch of handles
    let z_joiner = thread::spawn(move|| {
        zero.run_loop();
    });

    if let Ok(mut j) = joiners.lock() {
        j.push(z_joiner);
    } else {
        println!("Unable to get joiners lock.");
    }

    // Get all known readers so we can work on them later.
    match sqlite.lock() {
        Ok(sq) => {
            match sq.get_readers() {
                Ok(mut r) => {
                    if let Ok(mut reads) = readers.lock() {
                        reads.append(&mut r);
                    }
                },
                Err(e) => {
                    println!("Error getting readers mutex. {e}");
                    return
                },
            }
        }
        Err(e) => {
            println!("Error getting database mutex. {e}");
            return
        },
    }

    loop {
        if let Ok(ka) = keepalive.lock() {
            if *ka == false {
                break;
            }
        } else {
            println!("Error getting keep alive mutex. Exiting.");
            break;
        }
        match listener.accept() {
            Ok((stream, addr)) => {
                // set read_timeout for stream so we don't always block the entire time
                match stream.set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECONDS))) {
                    Ok(_) => (),
                    Err(e) => {
                        println!("Error setting read timeout: {e}");
                    }
                }
                println!("New connection: {}", addr);
                let t_stream = match stream.try_clone() {
                    Ok(st) => st,
                    Err(e) => {
                        println!("Error cloning stream. {e}");
                        continue
                    }
                };
                let t_keepalive = keepalive.clone();
                let t_controls = super::Control {
                    sighting_period: controls.sighting_period.clone(),
                    name: controls.name.clone(),
                    chip_type: controls.chip_type.clone(),
                    read_window: controls.read_window.clone(),                    
                };
                let t_readers = readers.clone();
                let t_joiners = joiners.clone();
                let t_read_repeaters = read_repeaters.clone();
                let t_sighting_repeaters = sighting_repeaters.clone();
                let t_sqlite = sqlite.clone();
                let t_control_sockets = control_sockets.clone();
                let mut placed = MAX_CONNECTED + 2;
                if let Ok(c_sock) = stream.try_clone() {
                    if let Ok(mut c_sockets) = control_sockets.lock() {
                        for i in 0..(MAX_CONNECTED + 1) {
                            if c_sockets[i].is_none() && i < MAX_CONNECTED {
                                c_sockets[i] = Some(c_sock);
                                placed = i;
                                break;
                            // Index MAX_CONNECTED is reserved for the system to tell itself to stop running in case of 
                            // power failure or some other reason the system needs to shut itself off.
                            } else if i == MAX_CONNECTED && addr.ip().is_loopback() && c_sockets[MAX_CONNECTED].is_none() {
                                c_sockets[i] = Some(c_sock);
                                placed = i;
                                break;
                            }
                        }
                    }
                    if placed <= MAX_CONNECTED {
                        let l_joiner = thread::spawn(move|| {
                            handle_stream(
                                placed,
                                t_stream,
                                t_keepalive,
                                t_controls,
                                &control_port,
                                t_readers,
                                t_joiners,
                                t_read_repeaters,
                                t_sighting_repeaters,
                                t_control_sockets,
                                t_sqlite
                            );
                        });
                        if let Ok(mut j) = joiners.lock() {
                            j.push(l_joiner);
                        } else {
                            println!("Unable to get joiners lock.");
                        }
                    } else {
                        _ = write_error(&stream, errors::Errors::TooManyConnections);
                    }
                } else {
                    _ = write_error(&stream, errors::Errors::ServerError{
                            message: String::from("unable to clone stream")
                    });
                }
            },
            Err(e) => {
                println!("Connection failed. {e}")
            }
        }
    }
}

fn handle_stream(
    index: usize,
    mut stream: TcpStream,
    keepalive: Arc<Mutex<bool>>,
    mut controls: super::Control,
    control_port: &u16,
    readers: Arc<Mutex<Vec<Box<dyn reader::Reader>>>>,
    joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
    read_reapeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
    sighting_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
    control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
) {
    println!("Starting control loop for index {index}");
    let mut data = [0 as u8; 51200];
    let mut no_error = true;
    let mut last_received_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    loop {
        if let Ok(ka) = keepalive.lock() {
            if *ka == false {
                break;
            }
        } else {
            println!("Error getting keep alive mutex. Exiting.");
            break;
        }
        let size = match stream.read(&mut data) {
            Ok(size) => size,
            Err(e) => {
                match e.kind() {
                    ErrorKind::TimedOut => {
                        0
                    },
                    _ => {
                        println!("Error reading from socket. {e}");
                        break;
                    }
                }
            },
        };
        if size > 0 {
            last_received_at = match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(time) => {
                    time.as_secs()
                },
                Err(e) => {
                    println!("Error getting time: {e}");
                    0
                }
            };
            let cmd: requests::Request = match serde_json::from_slice(&data[0..size]) {
                Ok(data) => {
                    println!("Received message: {:?}", data);
                    data
                },
                Err(e) => {
                    println!("Error deserializing request. {e}");
                    requests::Request::Unknown
                },
            };
            match cmd {
                requests::Request::Disconnect => {
                    // client requested to close the connection
                    _ = write_disconnect(&mut stream);
                    // tell then to close it and then break the loop to exit the thread
                    break;
                },
                requests::Request::Connect { reads, sightings } => {
                    let mut name = String::from("Unknown");
                    if let Ok(sq) = sqlite.lock() {
                        if let Ok(set) = sq.get_setting(SETTING_PORTAL_NAME) {
                            name = String::from(set.value())
                        }
                    }
                    if let Ok(mut repeaters) = read_reapeaters.lock() {
                        repeaters[index] = reads;
                    }
                    if let Ok(mut repeaters) = sighting_repeaters.lock() {
                        repeaters[index] = sightings;
                    }
                    if let Ok(u_readers) = readers.lock() {
                        no_error = write_connection_successful(&mut stream, name, reads, sightings, &u_readers);
                    } else {
                        no_error = write_error(&mut stream, errors::Errors::ServerError { message: String::from("unable to get readers mutex") })
                    }
                },
                requests::Request::KeepaliveAck => {

                },
                requests::Request::ReaderList => {
                    if let Ok(u_readers) = readers.lock() {
                        no_error = write_reader_list(&mut stream, &u_readers);
                    }
                },
                requests::Request::ReaderAdd { name, kind, ip_address, port } => {
                    if let Ok(sq) = sqlite.lock() {
                        match kind.as_str() {
                            reader::READER_KIND_ZEBRA => {
                                let port = if port < 100 {zebra::DEFAULT_ZEBRA_PORT} else {port};
                                let mut tmp = zebra::Zebra::new(
                                    0,
                                    name,
                                    ip_address,
                                    port,
                                    reader::AUTO_CONNECT_FALSE
                                );
                                match sq.save_reader(&tmp) {
                                    Ok(val) => {
                                        if let Ok(mut u_readers) = readers.lock() {
                                            match u_readers.iter().position(|x| x.nickname() == tmp.nickname()) {
                                                Some(ix) => {
                                                    let mut itmp = u_readers.remove(ix);
                                                    itmp.set_nickname(String::from(tmp.nickname()));
                                                    itmp.set_ip_address(String::from(tmp.ip_address()));
                                                    itmp.set_port(port);
                                                    u_readers.push(itmp);
                                                },
                                                None => {
                                                    tmp.set_id(val);
                                                    u_readers.push(Box::new(tmp));
                                                }
                                            }
                                            if let Ok(c_socks) = control_sockets.lock() {
                                                for sock in c_socks.iter() {
                                                    if let Some(sock) = sock {
                                                        // we might be writing to other sockets
                                                        // so errors here shouldn't close our connection
                                                        _ = write_reader_list(&sock, &u_readers);
                                                    }
                                                }
                                            } else {
                                                no_error = write_reader_list(&stream, &u_readers);
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error saving reader to database: {e}");
                                        no_error = write_error(&stream, errors::Errors::DatabaseError {
                                            message: format!("unexpected error saving reader to database: {e}"),
                                        });
                                    },
                                };
                            },
                            other => {
                                no_error = write_error(&stream, errors::Errors::InvalidReaderType {
                                    message: format!("'{}' is not a valid reader type. Valid Types: '{}'", other, reader::READER_KIND_ZEBRA)
                                 });
                            },
                        }
                    }
                },
                requests::Request::ReaderRemove { id } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_reader(&id) {
                            Ok(_) => {
                                if let Ok(mut u_readers) = readers.lock() {
                                    match u_readers.iter().position(|x| x.id() == id) {
                                        Some(ix) => {
                                            u_readers.remove(ix);
                                            ()
                                        },
                                        None => {},
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error removing database from reader: {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("unexpected error removing reader from database: {e}")
                                });
                            },
                        }
                    }
                    if let Ok(u_readers) = readers.lock() {
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                if let Some(sock) = sock {
                                    // we might be writing to other sockets
                                    // so errors here shouldn't close our connection
                                    _ = write_reader_list(&sock, &u_readers);
                                }
                            }
                        } else {
                            no_error = write_reader_list(&stream, &u_readers);
                        }
                    }
                },
                requests::Request::ReaderConnect { id } => {
                    if let Ok(mut u_readers) = readers.lock() {
                        match u_readers.iter().position(|x| x.id() == id) {
                            Some(ix) => {
                                let reader = u_readers.remove(ix);
                                match reader.kind() {
                                    reader::READER_KIND_ZEBRA => {
                                        let mut reader = reader::zebra::Zebra::new(
                                            reader.id(),
                                            String::from(reader.nickname()),
                                            String::from(reader.ip_address()),
                                            reader.port(),
                                            reader::AUTO_CONNECT_FALSE
                                        );
                                        match reader.connect(&sqlite, &controls) {
                                            Ok(j) => {
                                                if let Ok(mut join) = joiners.lock() {
                                                    join.push(j);
                                                }
                                            },
                                            Err(e) => {
                                                println!("Error connecting to reader: {e}");
                                                no_error = write_error(&stream, errors::Errors::ReaderConnection {
                                                    message: format!("error connecting to reader: {e}")
                                                });
                                            }
                                        }
                                        u_readers.push(Box::new(reader));
                                    },
                                    other => {
                                        no_error = write_error(&stream, errors::Errors::InvalidReaderType {
                                            message: format!("'{other}' reader type not yet implemented or invalid")
                                        });
                                        u_readers.push(reader);
                                    }
                                }
                            },
                            None => {
                                no_error = write_error(&stream, errors::Errors::NotFound);
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                if let Some(sock) = sock {
                                    no_error = write_reader_list(&sock, &u_readers) && no_error;
                                }
                            }
                        } else {
                            no_error = write_reader_list(&stream, &u_readers) && no_error;
                        }
                    }
                },
                requests::Request::ReaderDisconnect { id } => {
                    if let Ok(mut u_readers) = readers.lock() {
                        match u_readers.iter().position(|x| x.id() == id) {
                            Some(ix) => {
                                let mut reader = u_readers.remove(ix);
                                match reader.disconnect() {
                                    Ok(_) => {},
                                    Err(e) => {
                                        println!("Error connecting to reader: {e}");
                                        no_error = write_error(&stream, errors::Errors::ReaderConnection {
                                            message: format!("error discconnecting reader: {e}")
                                        });
                                    }
                                }
                                u_readers.push(reader);
                            },
                            None => {
                                no_error = write_error(&stream, errors::Errors::NotFound);
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                if let Some(sock) = sock {
                                    no_error = write_reader_list(&sock, &u_readers) && no_error;
                                }
                            }
                        } else {
                            no_error = write_reader_list(&stream, &u_readers) && no_error;
                        }
                    }
                },
                requests::Request::ReaderStart { id } => {
                    if let Ok(mut u_readers) = readers.lock() {
                        match u_readers.iter().position(|x| x.id() == id) {
                            Some(ix) => {
                                let mut reader = u_readers.remove(ix);
                                match reader.initialize() {
                                    Ok(_) => {},
                                    Err(e) => {
                                        println!("Error connecting to reader: {e}");
                                        no_error = write_error(&stream, errors::Errors::ReaderConnection {
                                            message: format!("error connecting to reader: {e}")
                                        });
                                    }
                                }
                                u_readers.push(reader);
                            },
                            None => {
                                no_error = write_error(&stream, errors::Errors::NotFound);
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                if let Some(sock) = sock {
                                    no_error = write_reader_list(&sock, &u_readers) && no_error;
                                }
                            }
                        } else {
                            no_error = write_reader_list(&stream, &u_readers) && no_error;
                        }
                    }
                },
                requests::Request::ReaderStop { id } => {
                    if let Ok(mut u_readers) = readers.lock() {
                        match u_readers.iter().position(|x| x.id() == id) {
                            Some(ix) => {
                                let mut reader = u_readers.remove(ix);
                                match reader.stop() {
                                    Ok(_) => {},
                                    Err(e) => {
                                        println!("Error connecting to reader: {e}");
                                        no_error = write_error(&stream, errors::Errors::ReaderConnection {
                                            message: format!("error connecting to reader: {e}")
                                        });
                                    }
                                }
                                u_readers.push(reader);
                            },
                            None => {
                                no_error = write_error(&stream, errors::Errors::NotFound);
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                if let Some(sock) = sock {
                                    no_error = write_reader_list(&sock, &u_readers) && no_error;
                                }
                            }
                        } else {
                            no_error = write_reader_list(&stream, &u_readers) && no_error;
                        }
                    }
                },
                requests::Request::SettingsGet => {
                    if let Ok(sq) = sqlite.lock() {
                        no_error = write_settings(&stream, &get_settings(&sq));
                    }
                },
                requests::Request::SettingsSet { settings } => {
                    for setting in settings {
                        match setting.name() {
                            super::SETTING_CHIP_TYPE |
                            super::SETTING_PORTAL_NAME |
                            super::SETTING_READ_WINDOW |
                            super::SETTING_SIGHTING_PERIOD => {
                                if let Ok(sq) = sqlite.lock() {
                                    match sq.set_setting(&setting) {
                                        Ok(_) => {
                                            controls = match super::Control::new(&sq) {
                                                Ok(c) => c,
                                                Err(e) => {
                                                    println!("error getting controls for some reason {e}");
                                                    controls
                                                }
                                            };
                                            let settings = get_settings(&sq);
                                            if let Ok(c_socks) = control_sockets.lock() {
                                                for sock in c_socks.iter() {
                                                    if let Some(sock) = sock {
                                                        // we might be writing to other sockets
                                                        // so errors here shouldn't close our connection
                                                        _ = write_settings(&sock, &settings);
                                                    }
                                                }
                                            } else {
                                                no_error = write_settings(&stream, &settings);
                                            }
                                        },
                                        Err(e) => {
                                            println!("Error saving setting. {e}");
                                            no_error = write_error(&stream, errors::Errors::DatabaseError {
                                                message: format!("error saving setting: {e}")
                                            });
                                        }
                                    }
                                }
                            },
                            other => {
                                println!("'{other}' is not a valid setting");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("'{other}' is not a valid setting")
                                });
                            }
                        }
                    }
                },
                requests::Request::Quit => {
                    if let Ok(mut ka) = keepalive.lock() {
                        *ka = false;
                    }
                    // connect to ensure the spawning thread will exit the accept call
                    _ = TcpStream::connect(format!("127.0.0.1:{}", control_port));
                },
                requests::Request::ApiList => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                no_error = write_api_list(&stream, &apis);
                            },
                            Err(e) => {
                                println!("error getting api list. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting api list: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ApiAdd { name, kind, uri, token } => {
                    match kind.as_str() {
                        api::API_TYPE_CHRONOKEEP_REMOTE |
                        api::API_TYPE_CKEEP_REMOTE_SELF |
                        api::API_TYPE_CHRONOKEEP_RESULTS |
                        api::API_TYPE_CKEEP_RESULTS_SELF => {
                            if let Ok(sq) = sqlite.lock() {
                                let t_uri = match kind.as_str() {
                                    api::API_TYPE_CHRONOKEEP_REMOTE => {
                                        String::from(api::API_URI_CHRONOKEEP_REMOTE)
                                    },
                                    api::API_TYPE_CHRONOKEEP_RESULTS => {
                                        String::from(api::API_URI_CHRONOKEEP_RESULTS)
                                    },
                                    _ => {
                                        uri
                                    }
                                };
                                match sq.save_api(&api::Api::new(
                                    0,
                                    name,
                                    kind,
                                    token,
                                    t_uri
                                )) {
                                    Ok(_) => {
                                        match sq.get_apis() {
                                            Ok(apis) => {
                                                if let Ok(c_socks) = control_sockets.lock() {
                                                    let mut count = 1;
                                                    for sock in c_socks.iter() {
                                                        if let Some(sock) = sock {
                                                            println!("writing to socket {count}");
                                                            count = count + 1;
                                                            // we might be writing to other sockets
                                                            // so errors here shouldn't close our connection
                                                            _ = write_api_list(&sock, &apis);
                                                        }
                                                    }
                                                } else {
                                                    no_error = write_api_list(&stream, &apis);
                                                }
                                            },
                                            Err(e) => {
                                                println!("error getting api list. {e}");
                                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                                    message: format!("error getting api list: {e}")
                                                });
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error saving api {e}");
                                        no_error = write_error(&stream, errors::Errors::DatabaseError {
                                            message: format!("error saving api {e}")
                                        });
                                    }
                                }
                            }
                        },
                        other => {
                            println!("'{other}' is not a valid api type");
                            no_error = write_error(&stream, errors::Errors::InvalidApiType {
                                message: format!("'{other}' is not a valid api type")
                            });
                        }
                    }
                },
                requests::Request::ApiRemove { name } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_api(&name) {
                            Ok(_) => {
                                match sq.get_apis() {
                                    Ok(apis) => {
                                        if let Ok(c_socks) = control_sockets.lock() {
                                            for sock in c_socks.iter() {
                                                if let Some(sock) = sock {
                                                    // we might be writing to other sockets
                                                    // so errors here shouldn't close our connection
                                                    _ = write_api_list(&sock, &apis);
                                                }
                                            }
                                        } else {
                                            no_error = write_api_list(&stream, &apis);
                                        }
                                    },
                                    Err(e) => {
                                        println!("error getting api list. {e}");
                                        no_error = write_error(&stream, errors::Errors::DatabaseError {
                                            message: format!("error getting api list: {e}")
                                        });
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error deleting api {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error deleting api: {e}")
                                });
                            }
                        }
                    }
                },
                /*
                requests::Request::ApiRemoteManualUpload { name } => {
                    // TODO
                },
                requests::Request::ApiRemoteAutoUpload { name } => {
                    // TODO
                },
                requests::Request::ApiResultsEventsGet { name } => {
                    // TODO
                },
                requests::Request::ApiResultsParticipantsGet { api_name, event_slug, event_year } => {
                    // TODO
                }, */
                requests::Request::ParticipantsGet => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_participants() {
                            Ok(parts) => {
                                no_error = write_participants(&stream, &parts);
                            },
                            Err(e) => {
                                println!("error getting participants from database. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting participants from database: {e}")
                                });
                            }
                        }
                    }
                }
                requests::Request::ParticipantsRemove => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_participants() {
                            Ok(_) => {
                                match sq.get_participants() {
                                    Ok(parts) => {
                                        if let Ok(c_socks) = control_sockets.lock() {
                                            for sock in c_socks.iter() {
                                                if let Some(sock) = sock {
                                                    // we might be writing to other sockets
                                                    // so errors here shouldn't close our connection
                                                    _ = write_participants(&sock, &parts);
                                                }
                                            }
                                        } else {
                                            no_error = write_participants(&stream, &parts);
                                        }
                                    },
                                    Err(e) => {
                                        println!("error getting participants. {e}");
                                        no_error = write_error(&stream, errors::Errors::DatabaseError {
                                            message: format!("error getting participants: {e}")
                                        });
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error deleting participants. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error deleting participants: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ReadsGet { start_seconds, end_seconds } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_reads(start_seconds, end_seconds) {
                            Ok(reads) => {
                                no_error = write_reads(&stream, &reads);
                            },
                            Err(e) => {
                                println!("Error getting reads. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting reads: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ReadsGetAll => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_all_reads() {
                            Ok(reads) => {
                                no_error = write_reads(&stream, &reads);
                            },
                            Err(e) => {
                                println!("Error getting reads. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting reads: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ReadsDelete { start_seconds, end_seconds } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_reads(start_seconds, end_seconds) {
                            Ok(count) => {
                                no_error = write_success(&stream, count);
                            },
                            Err(e) => {
                                println!("Error deleting reads. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error deleting reads: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ReadsDeleteAll => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_all_reads() {
                            Ok(count) => {
                                no_error = write_success(&stream, count);
                            },
                            Err(e) => {
                                println!("Error deleting reads. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error deleting reads: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::TimeGet => {
                    no_error = write_time(&stream);
                },
                /*
                requests::Request::TimeSet { time } => {
                    if on linux {
                        std::process::Command::new("COMMAND").arg("ARG").arg("ARG").spawn()
                    }
                }, */
                requests::Request::Subscribe { reads, sightings } => {
                    let mut message:String = String::from("");
                    if let Ok(mut repeaters) = read_reapeaters.lock() {
                        if (repeaters[index] == true && reads == true)
                        || (repeaters[index] == false && reads == false) {
                            message = format!("reads already set to {reads}")
                        } else {
                            repeaters[index] = reads
                        }
                    }
                    if let Ok(mut repeaters) = sighting_repeaters.lock() {
                        if (repeaters[index] == true && sightings == true)
                        || (repeaters[index] == false && sightings == false) {
                            message = if message.len() > 0 {format!("{message} sightings already set to {sightings}")} else {format!("sightings already set to {sightings}")}
                        } else {
                            repeaters[index] = sightings
                        }
                    }
                    if message.len() > 0 {
                        no_error = write_error(&stream, errors::Errors::AlreadySubscribed { message: message });
                    }
                },
                _ => {
                    no_error = write_error(&stream, errors::Errors::UnknownCommand)
                },
            }
        }
        if let Ok(time) = SystemTime::now().duration_since(UNIX_EPOCH) {
            // if we haven't received a message in 2 x the keep alive period then we've
            // probably disconnected
            if last_received_at + (2*KEEPALIVE_INTERVAL_SECONDS) < time.as_secs() {
                // write disconnect to tell the client what's going on if they're still
                // actually listening
                _ = write_disconnect(&stream);
                // and we can exit the loop because we're definitely disconnecting
                break;
            // send a keepalive message if we haven't heard from the socket in KEEPALIVE_INTERVAL_SECONDS
            } else if last_received_at + KEEPALIVE_INTERVAL_SECONDS < time.as_secs() {
                no_error = write_keepalive(&stream) && no_error;
            }
        }
        // check if we've encountered an error
        // exit the loop if we have
        if no_error == false {
            break;
        }
    }
    // if we've exited the loop we should ensure the program knows we can close this stream
    println!("Closing socket for index {index}.");
    // unsubscribe to notifications
    if let Ok(mut repeaters) = read_reapeaters.lock() {
        repeaters[index] = false;
    }
    if let Ok(mut repeaters) = sighting_repeaters.lock() {
        repeaters[index] = false;
    }
    stream.shutdown(Shutdown::Both).unwrap();
    if let Ok(mut c_socks) = control_sockets.lock() {
        c_socks[index] = None;
    }
}

fn get_available_port() -> u16 {
    match (4488..5588).find(|port| {
        match TcpListener::bind(("0.0.0.0", *port)) {
            Ok(_) => true,
            Err(_) => false,
        }
    }) {
        Some(port) => port,
        None => 0
    }
}

fn write_error(stream: &TcpStream, error: errors::Errors) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::Error{
        error,
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("1/ Something went wrong writing to socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("1/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

fn write_time(stream: &TcpStream) -> bool {
    let time = Utc::now();
    let utc = time.naive_utc();
    let local = time.naive_local();
    let output = match serde_json::to_writer(stream, &responses::Responses::Time{
        local: local.format("%Y-%m-%d %H:%M:%S").to_string(),
        utc: utc.format("%Y-%m-%d %H:%M:%S").to_string(),
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("2/ Something went wrong writing to socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("2/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

fn get_settings(sqlite: &MutexGuard<sqlite::SQLite>) -> Vec<setting::Setting> {
    let setting_names = [
        super::SETTING_CHIP_TYPE,
        super::SETTING_PORTAL_NAME,
        super::SETTING_READ_WINDOW,
        super::SETTING_SIGHTING_PERIOD,
    ];
    let mut settings: Vec<setting::Setting> = Vec::new();
    for name in setting_names {
        match sqlite.get_setting(name) {
            Ok(s) => {
                settings.push(s);
            },
            Err(_) => (),
        }
    }
    settings
}

fn write_settings(stream: &TcpStream, settings: &Vec<setting::Setting>) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::Settings{
        settings: settings.to_vec(),
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("3/ Something went wrong writing to socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("3/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

fn write_reader_list(stream: &TcpStream, u_readers: &MutexGuard<Vec<Box<dyn reader::Reader>>>) -> bool {
    let mut list: Vec<responses::Reader> = Vec::new();
    for r in u_readers.iter() {
        list.push(responses::Reader{
            id: r.id(),
            name: String::from(r.nickname()),
            kind: String::from(r.kind()),
            ip_address: String::from(r.ip_address()),
            port: r.port(),
            reading: r.is_reading(),
            connected: r.is_connected(),
        })
    };
    let output = match serde_json::to_writer(stream, &responses::Responses::Readers{
        readers: list,
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("4/ Something went wrong writing to socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("4/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

fn write_api_list(stream: &TcpStream, apis: &Vec<api::Api>) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::ApiList{
        apis: apis.to_vec()
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("5/ Something went wrong writing to socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("5/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

fn write_reads(stream: &TcpStream, reads: &Vec<read::Read>) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::Reads{
        list: reads.to_vec(),
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("6/ Something went wrong writing to socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("6/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

fn write_success(stream: &TcpStream, count: usize) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::Success {
        count
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("7/ Something went wrong writing to socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("7/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

fn write_participants(stream: &TcpStream, parts: &Vec<participant::Participant>) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::Participants {
        participants: parts.to_vec(),
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("8/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("8/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

fn write_connection_successful(stream: &TcpStream, name: String, reads: bool, sightings: bool, u_readers: &MutexGuard<Vec<Box<dyn reader::Reader>>>) -> bool {
    let mut list: Vec<responses::Reader> = Vec::new();
    for r in u_readers.iter() {
        list.push(responses::Reader{
            id: r.id(),
            name: String::from(r.nickname()),
            kind: String::from(r.kind()),
            ip_address: String::from(r.ip_address()),
            port: r.port(),
            reading: r.is_reading(),
            connected: r.is_connected(),
        })
    };
    let output = match serde_json::to_writer(stream, &responses::Responses::ConnectionSuccessful{
        name,
        kind: String::from(CONNECTION_TYPE),
        version: CONNECTION_VERS,
        reads_subscribed: reads,
        sightings_subscribed: sightings,
        readers: list
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("9/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("9/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

pub fn write_keepalive(stream: &TcpStream) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::Keepalive) {
        Ok(_) => true,
        Err(e) => {
            println!("10/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("10/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

pub fn write_disconnect(stream: &TcpStream) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::Disconnect) {
        Ok(_) => true,
        Err(e) => {
            println!("11/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("11/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}
use std::{thread::{JoinHandle, self}, sync::{Mutex, Arc, MutexGuard, Condvar}, net::{TcpListener, TcpStream, Shutdown, SocketAddr}, io::{Read, ErrorKind, Write}, time::{SystemTime, UNIX_EPOCH, Duration}, env};

use chrono::{Utc, Local, TimeZone};
use reqwest::header::{HeaderMap, CONTENT_TYPE, AUTHORIZATION};
use socket2::{Socket, Type, Protocol, Domain};

use crate::{database::{sqlite, Database}, reader::{self, zebra, auto_connect}, objects::{setting, participant, read, event::Event, sighting}, network::api::{self, Api}, control::{SETTING_PORTAL_NAME, socket::requests::AutoUploadQuery, sound}, results, processor, remote::{self, uploader}};

use super::zero_conf::ZeroConf;

pub mod requests;
pub mod responses;
pub mod errors;

pub const MAX_CONNECTED: usize = 4;
pub const CONNECTION_TYPE: &str = "chrono_portal";
pub const CONNECTION_VERS: usize = 1;

pub const READ_TIMEOUT_SECONDS: u64 = 5;
pub const KEEPALIVE_INTERVAL_SECONDS: u64 = 30;

pub const UPDATE_SCRIPT_ENV: &str = "PORTAL_UPDATE_SCRIPT";

pub fn control_loop(sqlite: Arc<Mutex<sqlite::SQLite>>, controls: super::Control) {
    // Keepalive is the boolean that tells us if we need to keep running.
    let keepalive: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));

    // Joiners are join handles for threads we spin up.
    let joiners: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    // Readers are chip readers that are saved.  They may be connected or reading as well.
    let readers: Arc<Mutex<Vec<reader::Reader>>> = Arc::new(Mutex::new(Vec::new()));

    // Control sockets are sockets that are connected and should be relayed any changes in settings / readers / apis
    // when another socket changes/deletes/adds something. -- Last spot is reserved for localhost to send shutdown command
    let control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>> = Arc::new(Mutex::new(Default::default()));
    // Read repeaters are sockets that want reads to be sent to them as they're being saved.
    let read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>> = Arc::new(Mutex::new([false;MAX_CONNECTED]));
    // Sighting repeaters are sockets that want sightings to be sent to them as they're being saved.
    let sighting_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>> = Arc::new(Mutex::new([false;MAX_CONNECTED]));
    
    // Our control port will be semi-random at the start to try to ensure we don't try to get a port in use.
    let control_port = get_available_port();

    let socket = match Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)) {
        Ok(sock) => sock,
        Err(e) => {
            println!("Error creating socket to listen to: {e}");
            return
        }
    };

    let address: SocketAddr = match format!("0.0.0.0:{control_port}").parse() {
        Ok(addr) => addr,
        Err(e) => {
            println!("Error getting address: {e}");
            return
        }
    };

    let address = address.into();
    // on windows specifically, SO_REUSEADDR must be set before bind or
    // it does not work
    match socket.set_reuse_address(true) {
        Ok(_) => {}
        Err(e) => {
            println!("Unable to set SO_REUSEADDR to true: {e}");
            return
        }
    }
    match socket.bind(&address) {
        Ok(_) => {
            println!("Control socket successfully bound.");
        }
        Err(e) => {
            println!("Error binding control socket: {e}");
            return
        }
    }
    match socket.listen(512) {
        Ok(_) => {},
        Err(e) => {
            println!("Socket Listen call failed: {e}");
            return
        }
    }
    let listener: TcpListener = socket.into();

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

    // create our sightings processing thread
    let sight_processor = Arc::new(processor::SightingsProcessor::new(
        control_sockets.clone(),
        sighting_repeaters.clone(),
        sqlite.clone(),
        keepalive.clone()
    ));
    let t_sight_processor = sight_processor.clone();
    let s_joiner = thread::spawn(move|| {
        t_sight_processor.start();
    });

    if let Ok(mut j) = joiners.lock() {
        j.push(s_joiner);
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

    // create our reads uploader struct for auto uploading if the user wants to
    let uploader = Arc::new(uploader::Uploader::new(keepalive.clone(), sqlite.clone()));

    // start a thread to play sounds if we are told we want to
    let sound_notifier = Arc::new(Condvar::new());
    let mut sound = sound::Sounds::new(
        controls.clone(),
        sound_notifier.clone(),
        keepalive.clone()
    );
    thread::spawn(move || {
        sound.run();
    });

    // create the auto connector for automatically connecting to readers
    let ac_state = Arc::new(Mutex::new(auto_connect::State::Unknown));
    let mut auto_connector = auto_connect::AutoConnector::new(
        ac_state.clone(),
        readers.clone(),
        joiners.clone(),
        control_sockets.clone(),
        read_repeaters.clone(),
        sight_processor.clone(),
        controls.clone(),
        sqlite.clone(),
        sound_notifier.clone()
    );
    // start a thread to automatically connect to readers
    thread::spawn(move|| {
        auto_connector.run();
    });

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
                    play_sound: controls.play_sound.clone(),
                    volume: controls.volume.clone(),
                };
                let t_readers = readers.clone();
                let t_joiners = joiners.clone();
                let t_read_repeaters = read_repeaters.clone();
                let t_sighting_repeaters = sighting_repeaters.clone();
                let t_sqlite = sqlite.clone();
                let t_control_sockets = control_sockets.clone();
                let t_sight_processor = sight_processor.clone();
                let t_uploader = uploader.clone();
                let t_ac_state = ac_state.clone();
                let t_sound_notifier = sound_notifier.clone();

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
                                t_sight_processor,
                                t_sqlite,
                                t_uploader,
                                t_ac_state,
                                t_sound_notifier
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
    println!("Shutting down control thread.");
    println!("Stopping readers.");
    if let Ok(mut r) = readers.lock() {
        for reader in r.iter_mut() {
            _ = reader.disconnect();
        }
    }
    println!("Stopping sightings processor.");
    sight_processor.stop();
    sight_processor.notify();
    println!("Joining all threads.");
    if let Ok(mut joiners) = joiners.lock() {
        while joiners.len() > 0 {
            let cur_thread = joiners.remove(0);
            match cur_thread.join() {
                Ok(_) => (),
                Err(e) => println!("Join failed. {:?}", e),
            }
        }
    };
    println!("Finished control thread shutdown.");
}

fn handle_stream(
    index: usize,
    mut stream: TcpStream,
    keepalive: Arc<Mutex<bool>>,
    mut controls: super::Control,
    control_port: &u16,
    readers: Arc<Mutex<Vec<reader::Reader>>>,
    joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
    read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
    sighting_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
    control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
    sight_processor: Arc<processor::SightingsProcessor>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    uploader: Arc<uploader::Uploader>,
    ac_state: Arc<Mutex<auto_connect::State>>,
    sound_notifier: Arc<Condvar>
) {
    println!("Starting control loop for index {index}");
    let mut data = [0 as u8; 51200];
    let mut no_error = true;
    let mut last_received_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let http_client = reqwest::blocking::Client::new();
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
                    ErrorKind::TimedOut |
                    ErrorKind::WouldBlock => {
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
                    match data {
                        requests::Request::KeepaliveAck => {},
                        requests::Request::TimeGet => {},
                        _ => {
                            println!("Received message: {:?}", data);
                        }
                    }
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
                    if let Ok(mut repeaters) = read_repeaters.lock() {
                        repeaters[index] = reads;
                    }
                    if let Ok(mut repeaters) = sighting_repeaters.lock() {
                        repeaters[index] = sightings;
                    }
                    if let Ok(u_readers) = readers.try_lock() {
                        no_error = write_connection_successful(&mut stream, name, reads, sightings, &u_readers);
                    } else {
                        no_error = write_error(&mut stream, errors::Errors::ServerError { message: String::from("unable to get readers mutex") })
                    }
                },
                requests::Request::KeepaliveAck => { },
                requests::Request::ReaderList => {
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown |
                            // the control app is only allowed to get a list of readers while the auto connect
                            // for readers is not finished (or before it's started)
                            auto_connect::State::Waiting => {
                                if let Ok(u_readers) = readers.lock() {
                                    no_error = write_reader_list(&mut stream, &u_readers);
                                }
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderAdd { name, kind, ip_address, port, auto_connect } => {
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
                                if let Ok(sq) = sqlite.lock() {
                                    let mut ac = reader::AUTO_CONNECT_FALSE;
                                    if auto_connect == true {
                                        ac = reader::AUTO_CONNECT_TRUE
                                    }
                                    match reader::Reader::new_no_repeaters(
                                        0,
                                        kind,
                                        name,
                                        ip_address,
                                        port,
                                        ac,
                                    ) {
                                        Ok(reader) => {
                                            let port = if port < 100 {zebra::DEFAULT_ZEBRA_PORT} else {port};
                                            let mut tmp = reader;
                                            match sq.save_reader(&tmp) {
                                                Ok(val) => {
                                                    if let Ok(mut u_readers) = readers.lock() {
                                                        match u_readers.iter().position(|x| x.nickname() == tmp.nickname()) {
                                                            Some(ix) => {
                                                                let mut itmp = u_readers.remove(ix);
                                                                itmp.set_nickname(String::from(tmp.nickname()));
                                                                itmp.set_ip_address(String::from(tmp.ip_address()));
                                                                itmp.set_port(port);
                                                                itmp.set_auto_connect(tmp.auto_connect());
                                                                u_readers.push(itmp);
                                                            },
                                                            None => {
                                                                tmp.set_id(val);
                                                                u_readers.push(tmp);
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
                                        Err(e) => {
                                            no_error = write_error(&stream, errors::Errors::InvalidReaderType {
                                                message: e.to_string()
                                             });
                                        },
                                    }
                                }
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderRemove { id } => {
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
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
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderConnect { id } => {
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
                                if let Ok(mut u_readers) = readers.lock() {
                                    match u_readers.iter().position(|x| x.id() == id) {
                                        Some(ix) => {
                                            let old_reader = u_readers.remove(ix);
                                            match reader::Reader::new(
                                                old_reader.id(),
                                                String::from(old_reader.kind()),
                                                String::from(old_reader.nickname()),
                                                String::from(old_reader.ip_address()),
                                                old_reader.port(),
                                                old_reader.auto_connect(),
                                                control_sockets.clone(),
                                                read_repeaters.clone(),
                                                sight_processor.clone(),
                                            ) {
                                                Ok(mut reader) => {
                                                    match reader.connect(&sqlite.clone(), &controls.clone(), sound_notifier.clone()) {
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
                                                    u_readers.push(reader);
                                                },
                                                Err(e) => {
                                                    no_error = write_error(&stream, errors::Errors::InvalidReaderType { message: e.to_string() });
                                                }
                                            };
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
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderDisconnect { id } => {
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
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
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderStart { id } => {
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
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
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderStop { id } => {
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
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
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        no_error = write_error(&stream, errors::Errors::StartingUp)
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
                            super::SETTING_SIGHTING_PERIOD |
                            super::SETTING_PLAY_SOUND |
                            super::SETTING_VOLUME => {
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
                        println!("Starting program stop sequence.");
                        *ka = false;
                    }
                    // connect to ensure the spawning thread will exit the accept call
                    _ = TcpStream::connect(format!("127.0.0.1:{}", control_port));
                },
                requests::Request::Shutdown => {
                    if let Ok(mut ka) = keepalive.lock() {
                        println!("Starting program stop sequence.");
                        *ka = false;
                    }
                    // connect to ensure the spawning thread will exit the accept call
                    _ = TcpStream::connect(format!("127.0.0.1:{}", control_port));
                    // send shutdown command to the OS
                    println!("Sending OS shutdown command if on Linux.");
                    match std::env::consts::OS {
                        "linux" => {
                            match std::process::Command::new("sudo").arg("shutdown").arg("now").spawn() {
                                Ok(_) => {
                                    println!("Shutdown command sent to OS successfully.");
                                },
                                Err(e) => {
                                    println!("Error sending shutdown command: {e}");
                                }
                            }
                        },
                        other => {
                            println!("Shutdown not supported on this platform ({other})");
                        }
                    }
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
                        api::API_TYPE_CHRONOKEEP_REMOTE_SELF => {
                            if let Ok(sq) = sqlite.lock() {
                                let t_uri = match kind.as_str() {
                                    api::API_TYPE_CHRONOKEEP_REMOTE => {
                                        String::from(api::API_URI_CHRONOKEEP_REMOTE)
                                    },
                                    _ => {
                                        uri
                                    }
                                };
                                match sq.get_apis() {
                                    Ok(apis) => {
                                        let mut remote_exists = false;
                                        for api in apis {
                                            if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                                remote_exists = true;
                                                break;
                                            }
                                        }
                                        if remote_exists {
                                            println!("Remote api already exists.");
                                            no_error = write_error(&stream, errors::Errors::TooManyRemoteApi)
                                        } else {
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
                                                    println!("Error saving api {e}");
                                                    no_error = write_error(&stream, errors::Errors::DatabaseError {
                                                        message: format!("error saving api: {e}")
                                                    });
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        println!("error getting api list. {e}");
                                        no_error = write_error(&stream, errors::Errors::DatabaseError {
                                            message: format!("error getting apis: {e}")
                                        })
                                    }
                                }
                            }
                        },
                        api::API_TYPE_CHRONOKEEP_RESULTS |
                        api::API_TYPE_CHRONOKEEP_RESULTS_SELF => {
                            if let Ok(sq) = sqlite.lock() {
                                let t_uri = match kind.as_str() {
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
                requests::Request::ApiRemoteManualUpload => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                let mut found = false;
                                for api in apis {
                                    if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                        found = true;
                                        if let Ok(sq) = sqlite.lock() {
                                            let reads = match sq.get_not_uploaded_reads() {
                                                Ok(it) => it,
                                                Err(e) => {
                                                    println!("Error geting reads to upload. {e}");
                                                    no_error = write_error(&stream, errors::Errors::DatabaseError { message: format!("error getting reads to upload: {e}") });
                                                    break;
                                                }
                                            };
                                            no_error = match upload_reads(&http_client, &api, &reads) {
                                                Ok(count) => {
                                                    write_success(&stream, count)
                                                },
                                                Err(e) => {
                                                    println!("Error uploading reads: {:?}", e);
                                                    write_error(&stream, e)
                                                }
                                            }
                                        } else {
                                            no_error = write_error(&stream, errors::Errors::ServerError { message: String::from("error getting database mutex") })
                                        }
                                        break;
                                    }
                                }
                                if found == false {
                                    no_error = write_error(&stream, errors::Errors::NoRemoteApi);
                                }
                            },
                            Err(e) => {
                                println!("error getting apis: {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting apis: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ApiRemoteAutoUpload { query } => {
                    match query {
                        AutoUploadQuery::Start => {
                            if uploader.running() {
                                no_error = write_error(&stream, errors::Errors::AlreadyRunning);
                            } else {
                                let t_uploader = uploader.clone();
                                let t_joiner = thread::spawn(move|| {
                                    t_uploader.run();
                                });
                                if let Ok(mut j) = joiners.lock() {
                                    j.push(t_joiner);
                                }
                                no_error = write_uploader_status(&stream, uploader.status());
                            }
                        }
                        AutoUploadQuery::Stop => {
                            if uploader.running() {
                                uploader.stop();
                                no_error = write_uploader_status(&stream, uploader.status());
                            } else {
                                no_error = write_error(&stream, errors::Errors::NotRunning);
                            }
                        }
                        AutoUploadQuery::Status => {
                            no_error = write_uploader_status(&stream, uploader.status());
                        }
                    }
                },
                requests::Request::ApiResultsEventsGet { api_name } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                for api in apis {
                                    if api.nickname() == api_name {
                                        if api.kind() == api::API_TYPE_CHRONOKEEP_RESULTS || api.kind() == api::API_TYPE_CHRONOKEEP_RESULTS_SELF {
                                            no_error = match get_events(&http_client, api) {
                                                Ok(events) => {
                                                    write_event_list(&stream, events)
                                                },
                                                Err(e) => {
                                                    println!("error getting events: {:?}", e);
                                                    write_error(&stream, e)
                                                }
                                            };
                                        } else {
                                            let kind = api.kind();
                                            println!("invalid api type specified: {kind}");
                                            no_error = write_error(&stream, errors::Errors::InvalidApiType { message: String::from("expected Chronokeep results type") })
                                        }
                                        break;
                                    }
                                }
                            },
                            Err(e) => {
                                println!("error getting apis from database: {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting participants from database: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ApiResultsEventYearsGet { api_name, event_slug } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                for api in apis {
                                    if api.nickname() == api_name {
                                        if api.kind() == api::API_TYPE_CHRONOKEEP_RESULTS || api.kind() == api::API_TYPE_CHRONOKEEP_RESULTS_SELF {
                                            no_error = match get_event_years(&http_client, api, event_slug) {
                                                Ok(years) => {
                                                    write_event_years(&stream, years)
                                                },
                                                Err(e) => {
                                                    println!("error getting event years: {:?}", e);
                                                    write_error(&stream, e)
                                                }
                                            };
                                        } else {
                                            let kind = api.kind();
                                            println!("invalid api type specified: {kind}");
                                            no_error = write_error(&stream, errors::Errors::InvalidApiType { message: String::from("expected Chronokeep results type") })
                                        }
                                        break;
                                    }
                                }
                            },
                            Err(e) => {
                                println!("error getting apis from database: {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting participants from database: {e}")
                                })
                            }
                        }
                    }
                },
                requests::Request::ApiResultsParticipantsGet { api_name, event_slug, event_year } => {
                    if let Ok(mut sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                for api in apis {
                                    if api.nickname() == api_name {
                                        if api.kind() == api::API_TYPE_CHRONOKEEP_RESULTS || api.kind() == api::API_TYPE_CHRONOKEEP_RESULTS_SELF {
                                            // try to get the participants from the API
                                            let new_parts = match get_participants(&http_client, api, event_slug, event_year) {
                                                Ok(new_parts) => {
                                                    new_parts
                                                },
                                                Err(e) => {
                                                    println!("error getting participants from api: {:?}", e);
                                                    no_error = write_error(&stream, e);
                                                    break;
                                                }
                                            };
                                            // delete old participants
                                            match sq.delete_participants() {
                                                Ok(_) => { },
                                                Err(e) => {
                                                    println!("error deleting participants: {e}");
                                                    no_error = write_error(&stream, errors::Errors::DatabaseError {
                                                        message: format!("error deleting participants: {e}")
                                                    });
                                                    break;
                                                }
                                            }
                                            // if participant deletion was successful, add new participants
                                            match sq.add_participants(&new_parts) {
                                                Ok(_) => { },
                                                Err(e) => {
                                                    println!("error adding participants: {e}");
                                                    no_error = write_error(&stream, errors::Errors::DatabaseError {
                                                        message: format!("error adding participants: {e}")
                                                    });
                                                    break;
                                                },
                                            }
                                            // get participants and send them to the connection that had us update participants
                                            match sq.get_participants() {
                                                Ok(parts) => {
                                                    no_error = write_participants(&stream, &parts)
                                                },
                                                Err(e) => {
                                                    println!("error getting participants: {e}");
                                                    no_error = write_error(&stream, errors::Errors::DatabaseError {
                                                        message: format!("error getting participants: {e}")
                                                    });
                                                }
                                            };
                                        } else {
                                            let kind = api.kind();
                                            println!("invalid api type specified: {kind}");
                                            no_error = write_error(&stream, errors::Errors::InvalidApiType { message: String::from("expected Chronokeep results type") })
                                        }
                                        break;
                                    }
                                }
                            },
                            Err(e) => {
                                println!("error getting apis from database: {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting participants from database: {e}")
                                })
                            }
                        }
                    }
                },
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
                requests::Request::ParticipantsAdd { participants } => {
                    if let Ok(mut sq) = sqlite.lock() {
                        match sq.add_participants(&participants) {
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
                                println!("Error adding participants. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error adding participants: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ReadsAdd { read } => {
                    if read.is_valid() == false {
                        no_error = write_error(&stream, errors::Errors::InvalidRead)
                    } else {
                        if let Ok(mut sq) = sqlite.lock() {
                            let mut reads: Vec<read::Read> = Vec::new();
                            reads.push(read);
                            match sq.save_reads(&reads) {
                                Ok(_) => {
                                    if let Ok(sockets) = control_sockets.lock() {
                                        if let Ok(repeaters) = read_repeaters.lock() {
                                            for ix in 0..MAX_CONNECTED {
                                                match &sockets[ix] {
                                                    Some(sock) => {
                                                        if repeaters[ix] == true {
                                                            no_error = no_error && write_reads(&sock, &reads);
                                                        }
                                                    },
                                                    None => {}
                                                }
                                            }
                                        }
                                    }
                                    sight_processor.notify();
                                },
                                Err(e) => {
                                    println!("Error saving manual read: {e}");
                                    no_error = write_error(&stream, errors::Errors::DatabaseError {
                                        message: format!("error saving manual read: {e}")
                                    });
                                }
                            }
                        }
                    }
                }
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
                requests::Request::SightingsGet { start_seconds, end_seconds } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_sightings(start_seconds, end_seconds) {
                            Ok(sightings) => {
                                no_error = write_sightings(&stream, &sightings)
                            },
                            Err(e) => {
                                println!("Error getting sightings. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting sightings: {e}")
                                })
                            }
                        }
                    }
                },
                requests::Request::SightingsGetAll => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_all_sightings() {
                            Ok(sightings) => {
                                no_error = write_sightings(&stream, &sightings)
                            },
                            Err(e) => {
                                println!("Error getting sightings. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting sightings: {e}")
                                })
                            }
                        }
                    }
                }
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
                requests::Request::SightingsDelete => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_sightings() {
                            Ok(count) => {
                                no_error = write_success(&stream, count);
                            }
                            Err(e) => {
                                println!("Error deleting sightings. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error deleting reads: {e}")
                                });
                            }
                        }
                    }
                }
                requests::Request::TimeGet => {
                    no_error = write_time(&stream);
                },
                requests::Request::TimeSet { time } => {
                    match std::env::consts::OS {
                        "linux" => {
                            match std::process::Command::new("sudo").arg("date").arg("-s").arg(format!("'{time}'")).spawn() {
                                Ok(_) => {
                                    no_error = write_time(&stream);
                                },
                                Err(e) => {
                                    println!("error setting time: {e}");
                                    no_error = write_error(&stream, errors::Errors::ServerError { message: format!("error setting time: {e}") })
                                }
                            }
                        },
                        other => {
                            println!("not supported on this platform ({other})");
                            no_error = write_error(&stream, errors::Errors::ServerError { message: format!("not supported on this platform ({other})") })
                        }
                    }
                },
                requests::Request::Subscribe { reads, sightings } => {
                    let mut message:String = String::from("");
                    if let Ok(mut repeaters) = read_repeaters.lock() {
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
                requests::Request::Update => {
                    match std::env::consts::OS {
                        "linux" => {
                            if let Ok(update_path) = env::var(UPDATE_SCRIPT_ENV) {
                                match std::process::Command::new(update_path).spawn() {
                                    Ok(_) => {
                                        no_error = write_success(&stream, 0);
                                    },
                                    Err(e) => {
                                        println!("error updating time: {e}");
                                        no_error = write_error(&stream, errors::Errors::ServerError { message: format!("error updating: {e}") })
                                    }
                                }
                            } else {
                                println!("update script environment variable not set");
                                no_error = write_error(&stream, errors::Errors::ServerError { message: String::from("update script environment variable not set") })
                            }
                        },
                        other => {
                            println!("not supported on this platform ({other})");
                            no_error = write_error(&stream, errors::Errors::ServerError { message: format!("not supported on this platform ({other})") })
                        }
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
    if let Ok(mut repeaters) = read_repeaters.lock() {
        repeaters[index] = false;
    }
    if let Ok(mut repeaters) = sighting_repeaters.lock() {
        repeaters[index] = false;
    }
    write_disconnect(&stream);
    _ = stream.shutdown(Shutdown::Both);
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
            println!("1/ Something went wrong writing to the socket. {e}");
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
    let local = Local.from_utc_datetime(&utc).format("%Y-%m-%d %H:%M:%S").to_string();
    let utc = utc.format("%Y-%m-%d %H:%M:%S").to_string();
    let output = match serde_json::to_writer(stream, &responses::Responses::Time{
        local,
        utc,
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("2/ Something went wrong writing to the socket. {e}");
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
        super::SETTING_PLAY_SOUND,
        super::SETTING_VOLUME
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
            println!("3/ Something went wrong writing to the socket. {e}");
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

pub fn write_reader_list(stream: &TcpStream, u_readers: &MutexGuard<Vec<reader::Reader>>) -> bool {
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
            auto_connect: r.auto_connect() == reader::AUTO_CONNECT_TRUE,
        })
    };
    let output = match serde_json::to_writer(stream, &responses::Responses::Readers{
        readers: list,
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("4/ Something went wrong writing to the socket. {e}");
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
            println!("5/ Something went wrong writing to the socket. {e}");
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

pub fn write_reads(stream: &TcpStream, reads: &Vec<read::Read>) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::Reads{
        list: reads.to_vec(),
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("6/ Something went wrong writing to the socket. {e}");
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

pub fn write_sightings(stream: &TcpStream, sightings: &Vec<sighting::Sighting>) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::Sightings {
        list: sightings.to_vec()
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("14/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("14/ Something went wrong writing to the socket. {e}");
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
            println!("7/ Something went wrong writing to the socket. {e}");
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

fn write_connection_successful(stream: &TcpStream, name: String, reads: bool, sightings: bool, u_readers: &MutexGuard<Vec<reader::Reader>>) -> bool {
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
            auto_connect: r.auto_connect() == reader::AUTO_CONNECT_TRUE,
        })
    };
    let mut updatable: bool = false;
    if let Ok(env) = env::var(UPDATE_SCRIPT_ENV) {
        if env.len() > 0 {
            updatable = true;
        }
    }
    let output = match serde_json::to_writer(stream, &responses::Responses::ConnectionSuccessful{
        name,
        kind: String::from(CONNECTION_TYPE),
        version: CONNECTION_VERS,
        reads_subscribed: reads,
        sightings_subscribed: sightings,
        readers: list,
        updatable: updatable,
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

pub fn write_event_list(stream: &TcpStream, events: Vec<Event>) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::Events {
        events
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("12/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("12/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

pub fn write_event_years(stream: &TcpStream, years: Vec<String>) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::EventYears { years }) {
        Ok(_) => true,
        Err(e) => {
            println!("13/ Something went wrong writing to the socket. {e}");
            return false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("13/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

pub fn write_uploader_status(stream: &TcpStream, status: uploader::Status) -> bool {
    let output = match serde_json::to_writer(stream, &responses::Responses::ReadAutoUpload {
        status
    }) {
        Ok(_) => true,
        Err(e) => {
            println!("15/ Something went wrong writing to the socket. {e}");
            return false
        }
    };
    let mut writer = stream;
    let output = output && match writer.write_all(b"\n") {
        Ok(_) => true,
        Err(e) => {
            println!("15/ Something went wrong writing to the socket. {e}");
            false
        }
    };
    output
}

fn construct_headers(key: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert(AUTHORIZATION, format!("Bearer {key}").parse().unwrap());
    headers
}

pub fn upload_reads(http_client: &reqwest::blocking::Client, api: &Api, reads: &[read::Read]) -> Result<usize, errors::Errors> {
    let url = api.uri();
    let response = match http_client.post(format!("{url}reads/add"))
        .headers(construct_headers(api.token()))
        .json(&remote::requests::UploadReadsRequest {
            reads: reads.to_vec()
        })
        .send() {
            Ok(resp) => resp,
            Err(e) => {
                println!("error trying to talk to api: {e}");
                return Err(errors::Errors::ServerError { message: format!("error trying to talk to api: {e}") })
            }
        };
    let output = match response.status() {
        reqwest::StatusCode::OK => {
            let resp_body: remote::responses::UploadReadsResponse = match response.json() {
                Ok(it) => it,
                Err(e) => {
                    println!("error trying to parse response from api: {e}");
                    return Err(errors::Errors::ServerError { message: format!("error trying to parse response from api: {e}") })
                }
            };
            resp_body.count
        },
        other => {
            println!("invalid status code: {other}");
            return Err(errors::Errors::ServerError { message: format!("invalid status code: {other}") })
        }
    };
    Ok(output)
}

fn get_events(http_client: &reqwest::blocking::Client, api: Api) -> Result<Vec<Event>, errors::Errors> {
    let url = api.uri();
    let response = match http_client.get(format!("{url}event/all"))
        .headers(construct_headers(api.token()))
        .send() {
            Ok(resp) => resp,
            Err(e) => {
                println!("error trying to talk to api: {e}");
                return Err(errors::Errors::ServerError { message: format!("error trying to talk to api: {e}") })
            }
        };
    let output = match response.status() {
        reqwest::StatusCode::OK => {
            let resp_body: results::responses::GetEventsResponse = match response.json() {
                Ok(it) => it,
                Err(e) => {
                    println!("error trying to parse response from api: {e}");
                    return Err(errors::Errors::ServerError { message: format!("error trying to parse response from api: {e}") })
                }
            };
            resp_body.events
        },
        reqwest::StatusCode::NOT_FOUND => {
            println!("event not found");
            return Err(errors::Errors::NotFound);
        }
        other => {
            println!("invalid status code: {other}");
            return Err(errors::Errors::ServerError { message: format!("invalid status code: {other}") })
        }
    };
    Ok(output)
}

fn get_event_years(http_client: &reqwest::blocking::Client, api: Api, slug: String) -> Result<Vec<String>, errors::Errors> {
    let url = api.uri();
    let response = match http_client.post(format!("{url}event"))
        .headers(construct_headers(api.token()))
        .json(&results::requests::GetEventRequest{
            slug,
        })
        .send() {
            Ok(resp) => resp,
            Err(e) => {
                println!("error trying to talk to api: {e}");
                return Err(errors::Errors::ServerError { message: format!("error trying to talk to api: {e}") })
            }
        };
    let output = match response.status() {
        reqwest::StatusCode::OK => {
            let resp_body: results::responses::GetEventResponse = match response.json() {
                Ok(it) => it,
                Err(e) => {
                    println!("error trying to parse response from api: {e}");
                    return Err(errors::Errors::ServerError { message: format!("error trying to parse response from api: {e}") })
                },
            };
            let mut years: Vec<String> = Vec::new();
            for y in resp_body.event_years {
                years.push(y.year);
            }
            years
        },
        reqwest::StatusCode::NOT_FOUND => {
            println!("event not found");
            return Err(errors::Errors::NotFound);
        }
        other => {
            println!("invalid status code: {other}");
            return Err(errors::Errors::ServerError { message: String::from("invalid status code") })
        }
    };
    Ok(output)
}

fn get_participants(http_client: &reqwest::blocking::Client,  api: Api, slug: String, year: String) -> Result<Vec<participant::Participant>, errors::Errors> {
    let url = api.uri();
    let response = match http_client.post(format!("{url}participants"))
        .headers(construct_headers(api.token()))
        .json(&results::requests::GetParticipantsRequest{
            slug,
            year
        })
        .send() {
            Ok(resp) => resp,
            Err(e) => {
                println!("error trying to talk to api: {e}");
                return Err(errors::Errors::ServerError { message: format!("error trying to talk to api: {e}") })
            }
        };
    let output = match response.status() {
        reqwest::StatusCode::OK => {
            let resp_body: results::responses::GetParticipantsResponse = match response.json() {
                Ok(it) => it,
                Err(e) => {
                    println!("error trying to parse response from api: {e}");
                    return Err(errors::Errors::ServerError { message: format!("error trying to parse response from api: {e}") })
                }
            };
            resp_body.participants
        },
        reqwest::StatusCode::NOT_FOUND => {
            println!("event not found");
            return Err(errors::Errors::NotFound);
        }
        other => {
            println!("invalid status code: {other}");
            return Err(errors::Errors::ServerError { message: String::from("invalid status code") })
        }
    };
    Ok(output)
}
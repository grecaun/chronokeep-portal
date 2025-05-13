use std::{env, io::{ErrorKind, Read, Write}, net::{Shutdown, SocketAddr, TcpListener, TcpStream}, sync::{Arc, Mutex, MutexGuard}, thread::{self, JoinHandle}, time::{Duration, SystemTime, UNIX_EPOCH}};
#[cfg(target_os = "linux")]
use crate::buttons::Buttons;
#[cfg(target_os = "linux")]
use crate::battery;

use chrono::{Local, TimeZone, Utc};
use reqwest::header::{HeaderMap, CONTENT_TYPE, AUTHORIZATION};
use socket2::{Socket, Type, Protocol, Domain};

use crate::{control::{socket::requests::AutoUploadQuery, sound::{self, SoundType}, SETTING_AUTO_REMOTE, SETTING_PORTAL_NAME}, database::{sqlite, Database}, network::api::{self, Api}, notifier::{self, Notifier}, objects::{bibchip, event::Event, participant, read, setting::{self, Setting}, sighting}, processor, reader::{self, auto_connect, reconnector::Reconnector, zebra, MAX_ANTENNAS}, remote::{self, remote_util, uploader::{self, Uploader}}, results, screen::CharacterDisplay, sound_board::Voice};

use self::notifications::APINotification;

use super::{sound::SoundNotifier, zero_conf::ZeroConf};

pub mod requests;
pub mod responses;
pub mod errors;
pub mod notifications;

pub const MAX_CONNECTED: usize = 4;
pub const CONNECTION_TYPE: &str = "chrono_portal";
pub const CONNECTION_VERS: usize = 1;

pub const CONNECTION_CHANGE_PAUSE: u64 = 500;

pub const READ_TIMEOUT_SECONDS: u64 = 5;
pub const KEEPALIVE_INTERVAL_SECONDS: u64 = 30;

pub const UPDATE_SCRIPT_ENV: &str = "PORTAL_UPDATE_SCRIPT";

pub const JSON_START_CHAR: char = '{';
pub const JSON_END_CHAR: char = '}';

pub fn control_loop(
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    control: &Arc<Mutex<super::Control>>,
    keepalive: Arc<Mutex<bool>>
) {
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
    let _ = socket.set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECONDS)));
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

    // Reset reads status on load and re-process sightings.
    if let Ok(sq) = sqlite.lock() {
        if let Err(e) = sq.reset_reads_status() {
            println!("Error trying to reset reads statuses: {e}");
        };
    }
    sight_processor.notify();

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

    // start a thread to play sounds if we are told we want to
    let mut sound = sound::Sounds::new(
        control.clone(),
        keepalive.clone()
    );
    let sound_notifier = sound.get_notifier();
    thread::spawn(move || {
        sound.run();
    });

    // start a thread to save reads from a reader so we don't tie up the readers when auto uploading
    let read_saver = Arc::new(processor::ReadSaver::new(
        sqlite.clone(),
        keepalive.clone()
    ));
    let z_read_saver = read_saver.clone();
    let rs_joiner = thread::spawn(move|| {
        z_read_saver.start();
    });

    if let Ok(mut j) = joiners.lock() {
        j.push(rs_joiner);
    } else {
        println!("Unable to get joiners lock.");
    }

    // Start a thread to enable notifications.
    let notifier = Notifier::new(keepalive.clone(), control.clone());
    let mut t_notifier = notifier.clone();
    let n_joiner = thread::spawn(move|| {
        t_notifier.run();
    });
    if let Ok(mut j) = joiners.lock() {
        j.push(n_joiner);
    }

    // create the auto connector for automatically connecting to readers
    let ac_state = Arc::new(Mutex::new(auto_connect::State::Unknown));
    let mut auto_connector = auto_connect::AutoConnector::new(
        ac_state.clone(),
        readers.clone(),
        joiners.clone(),
        control_sockets.clone(),
        read_repeaters.clone(),
        sight_processor.clone(),
        control.clone(),
        sqlite.clone(),
        read_saver.clone(),
        sound_notifier.clone(),
        notifier.clone(),
    );
    // start a thread to automatically connect to readers
    thread::spawn(move|| {
        auto_connector.run();
    });
    
    // Check if we can start a screen
    let screen: Arc<Mutex<Option<CharacterDisplay>>> = Arc::new(Mutex::new(None));
    // Check for screen information
    #[cfg(target_os = "linux")]
    {
        println!("Checking if there's a screen to display information on.");
        if let Ok(screen_bus) = std::env::var("PORTAL_SCREEN_BUS") {
            let bus: u8 = screen_bus.parse().unwrap_or(255);
            if bus < 50 {
                println!("Screen bus is {bus}.");
                if let Ok(mut screen) = screen.lock() {
                    let mut new_screen = CharacterDisplay::new(
                        keepalive.clone(),
                        control.clone(),
                        readers.clone(),
                        sqlite.clone(),
                        control_sockets.clone(),
                        read_repeaters.clone(),
                        sight_processor.clone(),
                        ac_state.clone(),
                        read_saver.clone(),
                        sound_notifier.clone(),
                        joiners.clone(),
                        control_port,
                        notifier.clone(),
                    );
                    *screen = Some(new_screen.clone());
                    thread::spawn(move|| {
                        new_screen.run(bus);
                    });
                }
                // Start buttons
                println!("Starting button thread.");
                let btns = Buttons::new(
                    screen.clone(), 
                    keepalive.clone()
                );
                thread::spawn(move|| {
                    btns.run();
                });
            }
        }
    }

    // create our reads uploader struct for auto uploading if the user wants to
    let uploader = Arc::new(uploader::Uploader::new(keepalive.clone(), sqlite.clone(), control_sockets.clone(), control.clone(), screen.clone()));
    if let Ok(control) = control.lock() {
        if control.auto_remote == true {
            println!("Starting auto upload thread.");
            let t_uploader = uploader.clone();
            let t_joiner = thread::spawn(move|| {
                t_uploader.run();
            });
            if let Ok(mut j) = joiners.lock() {
                j.push(t_joiner);
            }
        } else {
            if let Ok(mut screen) = screen.lock() {
                if let Some(screen) = &mut *screen {
                    screen.update_upload_status(uploader::Status::Stopped, 0);
                }
            }
        }
    };

    // Start our code to check the battery level.
    #[cfg(target_os = "linux")]
    let bat_check = battery::Checker::new(keepalive.clone(), control.clone(), screen.clone(), notifier.clone());
    #[cfg(target_os = "linux")]
    let b_joiner = thread::spawn(move|| {
        #[cfg(target_os = "linux")]
        bat_check.run();
    });
    #[cfg(target_os = "linux")]
    if let Ok(mut j) = joiners.lock() {
        j.push(b_joiner);
    }

    // Set screen on all the readers.
    if let Ok(mut u_readers) = readers.lock() {
        for reader in u_readers.iter_mut() {
            reader.set_screen(screen.clone());
            reader.set_control_sockets(control_sockets.clone());
            reader.set_readers(readers.clone());
        }
    }

    // play a sound to let the user know we've booted up fully and can accept control connections
    if let Ok(control) = control.lock() {
        if control.play_sound {
            control.sound_board.play_started(control.volume);
        }
    }

    notifier.send_notification(notifier::Notification::Start);

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
                let t_control = control.clone();
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
                let t_read_saver = read_saver.clone();
                let t_screen = screen.clone();
                let t_notifier = notifier.clone();

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
                                t_control,
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
                                t_read_saver,
                                t_sound_notifier,
                                t_screen,
                                t_notifier,
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
                if e.kind() != ErrorKind::WouldBlock {
                    println!("Connection failed. {e}")
                }
            }
        }
    }
    notifier.send_notification(notifier::Notification::Shutdown);
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
    println!("Finished control thread shutdown.");
    if let Ok(control) = control.lock() {
        if control.auto_remote {
            if let Ok(sq) = sqlite.lock() {
                match sq.get_apis() {
                    Ok(apis) => {
                        for api in apis {
                            if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                notifier.send_api_notification(&api, APINotification::ShuttingDown);
                                break;
                            }
                        }
                    },
                    Err(e) => {
                        println!("Error trying to get apis: {e}");
                    }
                }
            }
        }
    }
}

fn find_json_end(buffer: &String) -> usize {
    let chars = buffer.as_bytes();
    let mut start_count = 0;
    let mut ix = 0;
    // get to the first start char
    while ix < chars.len() {
        if chars[ix] == JSON_START_CHAR as u8 {
            start_count += 1;
            ix += 1;
            break;
        }
        ix += 1;
    }
    if ix >= chars.len() - 1 {
        return chars.len() - 1;
    }
    while ix < chars.len() {
        if chars[ix] == JSON_START_CHAR as u8 {
            start_count += 1;
        } else if chars[ix] == JSON_END_CHAR as u8 {
            start_count -= 1;
            if start_count < 1 {
                if ix + 1 < chars.len() && chars[ix + 1] == '\n' as u8 {
                    return ix + 1;
                }
                return ix;
            }
        }
        ix += 1;
    }
    return chars.len() - 1;
}

fn handle_stream(
    index: usize,
    mut stream: TcpStream,
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<super::Control>>,
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
    read_saver: Arc<processor::ReadSaver>,
    sound: Arc<SoundNotifier>,
    screen: Arc<Mutex<Option<CharacterDisplay>>>,
    notifier: notifier::Notifier,
) {
    println!("Starting control loop for index {index}");
    let mut data = [0 as u8; 51200];
    let mut buffer = String::new();
    let mut no_error = true;
    let mut last_received_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let http_client = reqwest::blocking::ClientBuilder::new().timeout(Duration::from_secs(30))
                                .connect_timeout(Duration::from_secs(30)).build()
                                .unwrap_or(reqwest::blocking::Client::new());
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
            let s = match std::str::from_utf8(&data[0..size]) {
                Ok(buf) => buf,
                Err(e) => {
                    println!("Error parsing data received: {e}");
                    ""
                },
            };
            buffer.push_str(&s);
        }
        while buffer.len() > 0
        {
            // custom parse //
            let mut newline = find_json_end(&buffer);
            if newline < buffer.len() {
                newline += 1;
            }
            let single_line: String = buffer.drain(..newline).collect();
            let cmd: requests::Request = match serde_json::from_str(&single_line) {
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
                    if buffer.len() == 0 {
                        buffer.push_str(&single_line);
                        break;
                    } else {
                        println!("Error deserializing request. {e}");
                    }
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
                        no_error = write_connection_successful(&mut stream, name, reads, sightings, &*u_readers, &uploader);
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
                                    no_error = write_reader_list(&mut stream, &*u_readers);
                                }
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderAdd { id, name, kind, ip_address, port, auto_connect } => {
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
                                        id,
                                        kind,
                                        name,
                                        ip_address,
                                        port,
                                        ac,
                                    ) {
                                        Ok(reader) => {
                                            let port = if port < 100 {zebra::DEFAULT_ZEBRA_PORT} else {port};
                                            let mut tmp = reader;
                                            tmp.set_screen(screen.clone());
                                            match sq.save_reader(&tmp) {
                                                Ok(val) => {
                                                    if let Ok(mut u_readers) = readers.lock() {
                                                        match u_readers.iter().position(|x| x.id() == tmp.id() || x.nickname().eq_ignore_ascii_case(tmp.nickname())) {
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
                                                                    _ = write_reader_list(&sock, &*u_readers);
                                                                }
                                                            }
                                                        } else {
                                                            no_error = write_reader_list(&stream, &*u_readers);
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
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
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
                                                _ = write_reader_list(&sock, &*u_readers);
                                            }
                                        }
                                    } else {
                                        no_error = write_reader_list(&stream, &*u_readers);
                                    }
                                }
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderConnect { id } | requests::Request::ReaderStart { id } => {
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
                                if let Ok(mut u_readers) = readers.lock() {
                                    match u_readers.iter().position(|x| x.id() == id) {
                                        Some(ix) => {
                                            let old_reader = u_readers.remove(ix);
                                            if old_reader.is_connected() != Some(true) {
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
                                                    screen.clone(),
                                                    readers.clone(),
                                                ) {
                                                    Ok(mut reader) => {
                                                        let reconnector = Reconnector::new(
                                                            readers.clone(),
                                                            joiners.clone(),
                                                            control_sockets.clone(),
                                                            read_repeaters.clone(),
                                                            sight_processor.clone(),
                                                            control.clone(),
                                                            sqlite.clone(),
                                                            read_saver.clone(),
                                                            sound.clone(),
                                                            reader.id(),
                                                            1,
                                                            notifier.clone(),
                                                        );
                                                        match reader.connect(
                                                                &sqlite.clone(),
                                                                &control.clone(),
                                                                &read_saver.clone(),
                                                                sound.clone(),
                                                                Some(reconnector),
                                                                notifier.clone(),
                                                            ) {
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
                                                        u_readers.push(old_reader);
                                                        no_error = write_error(&stream, errors::Errors::InvalidReaderType { message: e.to_string() });
                                                    }
                                                };
                                            } else {
                                                no_error = write_error(&stream, errors::Errors::AlreadyRunning);
                                            }
                                        },
                                        None => {
                                            no_error = write_error(&stream, errors::Errors::NotFound);
                                        }
                                    };
                                }
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                    #[cfg(target_os = "linux")]
                    if let Ok(mut screen_opt) = screen.lock() {
                        if let Some(screen) = &mut *screen_opt {
                            screen.update();
                        }
                    }
                },
                requests::Request::ReaderDisconnect { id } | requests::Request::ReaderStop { id }  => {
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
                                                        message: format!("error stopping reader: {e}")
                                                    });
                                                }
                                            }
                                            match reader.disconnect() {
                                                Ok(_) => {},
                                                Err(e) => {
                                                    println!("Error connecting to reader: {e}");
                                                    no_error = write_error(&stream, errors::Errors::ReaderConnection {
                                                        message: format!("error disconnecting reader: {e}")
                                                    });
                                                }
                                            }
                                            u_readers.push(reader);
                                        },
                                        None => {
                                            no_error = write_error(&stream, errors::Errors::NotFound);
                                        }
                                    };
                                    thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                    if let Ok(c_socks) = control_sockets.lock() {
                                        for sock in c_socks.iter() {
                                            if let Some(sock) = sock {
                                                no_error = write_reader_list(&sock, &*u_readers) && no_error;
                                            }
                                        }
                                    } else {
                                        no_error = write_reader_list(&stream, &*u_readers) && no_error;
                                    }
                                }
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                    #[cfg(target_os = "linux")]
                    if let Ok(mut screen_opt) = screen.lock() {
                        if let Some(screen) = &mut *screen_opt {
                            screen.update();
                        }
                    }
                },
                requests::Request::ReaderStartAll => { // START ALL
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
                                if let Ok(mut u_readers) = readers.lock() {
                                    // make sure to iterate through the vec in reverse so we don't have some weird loop issues
                                    for ix in (0..u_readers.len()).rev() {
                                        let mut reader = u_readers.remove(ix);
                                        if reader.is_connected() != Some(true) {
                                            reader.set_control_sockets(control_sockets.clone());
                                            reader.set_readers(readers.clone());
                                            reader.set_read_repeaters(read_repeaters.clone());
                                            reader.set_sight_processor(sight_processor.clone());
                                            reader.set_screen(screen.clone());
                                            let reconnector = Reconnector::new(
                                                readers.clone(),
                                                joiners.clone(),
                                                control_sockets.clone(),
                                                read_repeaters.clone(),
                                                sight_processor.clone(),
                                                control.clone(),
                                                sqlite.clone(),
                                                read_saver.clone(),
                                                sound.clone(),
                                                reader.id(),
                                                1,
                                                notifier.clone(),
                                            );
                                            match reader.connect(
                                                    &sqlite.clone(), 
                                                    &control.clone(),
                                                    &read_saver.clone(),
                                                    sound.clone(),
                                                    Some(reconnector),
                                                    notifier.clone(),
                                                ) {
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
                                        }
                                        u_readers.push(reader);
                                    }
                                    thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                    if let Ok(c_socks) = control_sockets.lock() {
                                        for sock in c_socks.iter() {
                                            if let Some(sock) = sock {
                                                no_error = write_reader_list(&sock, &*u_readers) && no_error;
                                            }
                                        }
                                    } else {
                                        no_error = write_reader_list(&stream, &*u_readers) && no_error;
                                    }
                                }
                            },
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&stream, errors::Errors::StartingUp);
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                    #[cfg(target_os = "linux")]
                    if let Ok(mut screen_opt) = screen.lock() {
                        if let Some(screen) = &mut *screen_opt {
                            screen.update();
                        }
                    }
                },
                requests::Request::ReaderStopAll => {  // STOP ALL
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
                                if let Ok(mut u_readers) = readers.lock() {
                                    for ix in (0..u_readers.len()).rev() {
                                        let mut reader = u_readers.remove(ix);
                                        if reader.is_reading() == Some(true) {
                                            match reader.stop() {
                                                Ok(_) => {},
                                                Err(e) => {
                                                    println!("Error connecting to reader: {e}");
                                                    no_error = write_error(&stream, errors::Errors::ReaderConnection {
                                                        message: format!("error stopping reader: {e}")
                                                    });
                                                }
                                            }
                                        }
                                        if reader.is_connected() == Some(true) {
                                            match reader.disconnect() {
                                                Ok(_) => {},
                                                Err(e) => {
                                                    println!("Error connecting to reader: {e}");
                                                    no_error = write_error(&stream, errors::Errors::ReaderConnection {
                                                        message: format!("error discconnecting reader: {e}")
                                                    });
                                                }
                                            }
                                        }
                                        u_readers.push(reader);
                                    }
                                    thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                    if let Ok(c_socks) = control_sockets.lock() {
                                        for sock in c_socks.iter() {
                                            if let Some(sock) = sock {
                                                no_error = write_reader_list(&sock, &*u_readers) && no_error;
                                            }
                                        }
                                    } else {
                                        no_error = write_reader_list(&stream, &*u_readers) && no_error;
                                    }
                                }
                            },
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&stream, errors::Errors::StartingUp)
                            },
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&stream, errors::Errors::StartingUp)
                    }
                    #[cfg(target_os = "linux")]
                    if let Ok(mut screen_opt) = screen.lock() {
                        if let Some(screen) = &mut *screen_opt {
                            screen.update();
                        }
                    }
                },
                requests::Request::ReaderGetAll => {
                    if let Ok(u_readers) = readers.lock() {
                        no_error = write_reader_list(&stream, &*u_readers) && no_error;
                    }
                }
                requests::Request::SettingsGet => {
                    if let Ok(sq) = sqlite.lock() {
                        no_error = write_settings(&stream, &get_settings(&sq));
                    }
                },
                requests::Request::SettingsGetAll => {
                    if let Ok(sq) = sqlite.lock() {
                        let settings = get_settings(&sq);
                        match sq.get_apis() {
                            Ok(apis) => {
                                if let Ok(u_readers) = readers.lock() {
                                    no_error = write_all_settings(&stream, &settings, &*u_readers, &apis, uploader.status());
                                } else {
                                    no_error = write_error(&stream, errors::Errors::ServerError { message: String::from("error getting the readers mutex") });
                                }
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
                requests::Request::SettingsSet { settings } => {
                    if let Ok(mut control) = control.lock() {
                        let old_volume = control.volume;
                        let old_play_sound = control.play_sound;
                        let old_voice = control.sound_board.get_voice();
                        let mut custom_error = false;
                        for setting in settings {
                            match setting.name() {
                                super::SETTING_VOICE => {
                                    let new_voice = Voice::from_str(setting.value());
                                    if new_voice == Voice::Custom && !control.sound_board.custom_available() {
                                        println!("Custom voice selected but not available.");
                                        custom_error = true;
                                    } else {
                                        if let Ok(sq) = sqlite.lock() {
                                            match sq.set_setting(&setting) {
                                                Ok(_) => {
                                                    if let Ok(new_control) = super::Control::new(&sq) {
                                                        _ = control.update(new_control);
                                                    } else {
                                                        let settings = get_settings(&sq);
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
                                    }
                                },
                                super::SETTING_CHIP_TYPE |
                                super::SETTING_PORTAL_NAME |
                                super::SETTING_READ_WINDOW |
                                super::SETTING_SIGHTING_PERIOD |
                                super::SETTING_PLAY_SOUND |
                                super::SETTING_UPLOAD_INTERVAL |
                                super::SETTING_VOLUME |
                                super::SETTING_NTFY_URL |
                                super::SETTING_NTFY_USER |
                                super::SETTING_NTFY_PASS |
                                super::SETTING_NTFY_TOPIC => {
                                    if let Ok(sq) = sqlite.lock() {
                                        match sq.set_setting(&setting) {
                                            Ok(_) => {
                                                if let Ok(new_control) = super::Control::new(&sq) {
                                                    _ = control.update(new_control);
                                                    
                                                } else {
                                                    let settings = get_settings(&sq);
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
                        if let Ok(sq) = sqlite.lock() {
                            let settings = get_settings(&sq);
                            if let Ok(c_socks) = control_sockets.lock() {
                                for sock in c_socks.iter() {
                                    if let Some(sock) = sock {
                                        // we might be writing to other sockets
                                        // so errors here shouldn't close our connection
                                        _ = write_settings(&sock, &settings);
                                    }
                                }
                            }
                        }
                        if custom_error && control.play_sound {
                            sound.notify_custom(SoundType::CustomNotAvailable);
                        } else if old_voice != control.sound_board.get_voice() && control.play_sound  {
                            sound.notify_custom(SoundType::Introduction);
                        }
                        if (old_play_sound != control.play_sound || old_volume != control.volume) && control.play_sound {
                            sound.notify_custom(SoundType::Volume);
                        }
                    } else {
                        if let Ok(sq) = sqlite.lock() {
                            let settings = get_settings(&sq);
                            no_error = write_settings(&stream, &settings);
                        }
                    }
                    #[cfg(target_os = "linux")]
                    if let Ok(mut screen_opt) = screen.lock() {
                        if let Some(screen) = &mut *screen_opt {
                            screen.update_settings();
                            screen.update();
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
                    #[cfg(target_os = "linux")]
                    if let Ok(mut screen_opt) = screen.lock() {
                        if let Some(screen) = &mut *screen_opt {
                            screen.update();
                        }
                    }
                },
                requests::Request::Shutdown => {
                    if let Ok(mut ka) = keepalive.lock() {
                        println!("Starting program stop sequence.");
                        *ka = false;
                    }
                    // play a shutdown command since the shutdown 
                    if let Ok(control) = control.lock() {
                        if control.play_sound {
                            control.sound_board.play_shutdown(control.volume);
                        }
                    }
                    // send shutdown command to the OS
                    println!("Sending OS shutdown command if on Linux.");
                    match std::env::consts::OS {
                        "linux" => {
                            match std::process::Command::new("sudo").arg("shutdown").arg("-h").arg("now").spawn() {
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
                    // connect to ensure the spawning thread will exit the accept call
                    _ = TcpStream::connect(format!("127.0.0.1:{}", control_port));
                    #[cfg(target_os = "linux")]
                    if let Ok(mut screen_opt) = screen.lock() {
                        if let Some(screen) = &mut *screen_opt {
                            screen.set_shutdown();
                            screen.update();
                        }
                    }
                },
                requests::Request::Restart => {
                    if let Ok(mut ka) = keepalive.lock() {
                        println!("Starting program stop sequence.");
                        *ka = false;
                    }
                    if let Ok(control) = control.lock() {
                        if control.play_sound {
                            control.sound_board.play_shutdown(control.volume);
                        }
                    }
                    println!("Sending restart command if on Linux.");
                    match std::env::consts::OS {
                        "linux" => {
                            match std::process::Command::new("sudo").arg("systemctl").arg("restart").arg("portal").spawn() {
                                Ok(_) => {
                                    println!("Restart command sent to OS successfully.");
                                },
                                Err(e) => {
                                    println!("Error sending restart command: {e}");
                                }
                            }
                        },
                        other => {
                            println!("Restart not supported on this platform ({other})");
                        }
                    }
                    // connect to ensure the spawning thread will exit the accept call
                    _ = TcpStream::connect(format!("127.0.0.1:{}", control_port));
                    #[cfg(target_os = "linux")]
                    if let Ok(mut screen_opt) = screen.lock() {
                        if let Some(screen) = &mut *screen_opt {
                            screen.set_shutdown();
                            screen.update();
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
                requests::Request::ApiSave { id, name, kind, uri, token } => {
                    match kind.as_str() {
                        api::API_TYPE_CHRONOKEEP_REMOTE |
                        api::API_TYPE_CHRONOKEEP_REMOTE_SELF => {
                            if let Ok(sq) = sqlite.lock() {
                                let mut t_uri = match kind.as_str() {
                                    api::API_TYPE_CHRONOKEEP_REMOTE => {
                                        String::from(api::API_URI_CHRONOKEEP_REMOTE)
                                    },
                                    _ => {
                                        uri
                                    }
                                };
                                if !t_uri.ends_with("/") {
                                    t_uri = format!("{t_uri}/")
                                }
                                match sq.get_apis() {
                                    Ok(apis) => {
                                        let mut remote_exists = false;
                                        for api in apis {
                                            if (api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF) && api.id() != id  {
                                                remote_exists = true;
                                                break;
                                            }
                                        }
                                        if remote_exists {
                                            println!("Remote api already exists.");
                                            no_error = write_error(&stream, errors::Errors::TooManyRemoteApi)
                                        } else {
                                            match sq.save_api(&api::Api::new(
                                                id,
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
                                let mut t_uri = match kind.as_str() {
                                    api::API_TYPE_CHRONOKEEP_RESULTS => {
                                        String::from(api::API_URI_CHRONOKEEP_RESULTS)
                                    },
                                    _ => {
                                        uri
                                    }
                                };
                                if !t_uri.ends_with("/") {
                                    t_uri = format!("{t_uri}/")
                                }
                                match sq.save_api(&api::Api::new(
                                    id,
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
                requests::Request::ApiSaveAll { list } => {
                    if let Ok(sq) = sqlite.lock() {
                        let mut remote_api: Option<Api> = None;
                        // check if we have a remote api already set, there can only be one
                        match sq.get_apis() {
                            Ok(apis) => {
                                for api in apis {
                                    if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                        remote_api = Some(api);
                                        break;
                                    }
                                }
                            },
                            Err(e) => {
                                println!("error getting api list. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting apis: {e}")
                                })
                            }
                        }
                        // if we've got a remote api set we need to check if there is another
                        if let Some(remote) = remote_api {
                            let mut remote_exists = false;
                            let mut invalid_type = false;
                            for api in &list {
                                match api.kind() {
                                    api::API_TYPE_CHRONOKEEP_REMOTE |
                                    api::API_TYPE_CHRONOKEEP_REMOTE_SELF => {
                                        // if the id's don't match then they're trying to save a second
                                        if remote.id() != api.id() {
                                            remote_exists = true;
                                        }
                                    },
                                    api::API_TYPE_CHRONOKEEP_RESULTS |
                                    api::API_TYPE_CHRONOKEEP_RESULTS_SELF => {},
                                    _ => {
                                        invalid_type = true;
                                    }
                                }
                            }
                            // if we found a duplicate remote, don't save and write error
                            if remote_exists {
                                println!("Remote api already exists.");
                                no_error = write_error(&stream, errors::Errors::TooManyRemoteApi);
                            // if there's an invalid type, don't save and write error
                            } else if invalid_type {
                                println!("One or more invalid api types found.");
                                no_error = write_error(&stream, errors::Errors::InvalidApiType { message: String::from("one or more invalid api types found") });
                            // all are saveable
                            } else {
                                let mut error_saving = false;
                                // check if we have any errors saving apis
                                for api in list {
                                    let mut t_uri = match api.kind() {
                                        api::API_TYPE_CHRONOKEEP_RESULTS => {
                                            String::from(api::API_URI_CHRONOKEEP_RESULTS)
                                        },
                                        _ => {
                                            String::from(api.uri())
                                        }
                                    };
                                    if !t_uri.ends_with("/") {
                                        t_uri = format!("{t_uri}/")
                                    }
                                    match sq.save_api(&api::Api::new(
                                        api.id(),
                                        String::from(api.nickname()),
                                        String::from(api.kind()),
                                        String::from(api.token()),
                                        t_uri
                                    )) {
                                        Ok(_) => { },
                                        Err(_) => {
                                            error_saving = true;
                                        }
                                    }
                                }
                                // write an error message if we had an issue
                                if error_saving {
                                    println!("Error saving one or more apis");
                                    no_error = write_error(&stream, errors::Errors::DatabaseError {
                                        message: String::from("error saving one or more apis")
                                    });
                                // otherwise send everyone connected the updated list of apis
                                } else {
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
                                }
                            }
                        // no previous remote api found
                        } else {
                            let mut remote_count = 0;
                            let mut invalid_type = false;
                            for api in &list {
                                match api.kind() {
                                    api::API_TYPE_CHRONOKEEP_REMOTE |
                                    api::API_TYPE_CHRONOKEEP_REMOTE_SELF => {
                                        remote_count += 1
                                    },
                                    api::API_TYPE_CHRONOKEEP_RESULTS |
                                    api::API_TYPE_CHRONOKEEP_RESULTS_SELF => {},
                                    _ => {
                                        invalid_type = true;
                                    }
                                }
                            }
                            // if we found a duplicate remote, don't save and write error
                            if remote_count > 1 {
                                println!("Remote api already exists.");
                                no_error = write_error(&stream, errors::Errors::TooManyRemoteApi);
                            // if there's an invalid type, don't save and write error
                            } else if invalid_type {
                                println!("One or more invalid api types found.");
                                no_error = write_error(&stream, errors::Errors::InvalidApiType { message: String::from("one or more invalid api types found") });
                            // all are saveable
                            } else {
                                let mut error_saving = false;
                                // check if we have any errors saving apis
                                for api in list {
                                    match sq.save_api(&api) {
                                        Ok(_) => { },
                                        Err(_) => {
                                            error_saving = true;
                                        }
                                    }
                                }
                                // write an error message if we had an issue
                                if error_saving {
                                    println!("Error saving one or more apis");
                                    no_error = write_error(&stream, errors::Errors::DatabaseError {
                                        message: String::from("error saving one or more apis")
                                    });
                                // otherwise send everyone connected the updated list of apis
                                } else {
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
                                }
                            }
                        }
                    }
                },
                requests::Request::ApiRemove { id } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_api(&id) {
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
                    let mut to_upload: Vec<read::Read> = Vec::new();
                    let mut upload_api: Option<api::Api> = None;
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                let mut found = false;
                                for api in apis {
                                    if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                        found = true;
                                        upload_api = Some(api.clone());
                                        //println!("Uploading reads to {}", api.nickname());
                                        // this request will upload all reads regardless of whether or not they've been uploaded previously
                                        match sq.get_all_reads() {
                                            Ok(mut reads) => {
                                                to_upload.append(&mut reads);
                                            },
                                            Err(e) => {
                                                println!("Error geting reads to upload. {e}");
                                                no_error = write_error(&stream, errors::Errors::DatabaseError { message: format!("error getting reads to upload: {e}") });
                                                break;
                                            }
                                        };
                                        // remote api found, so break the loop looking for it
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
                    // upload any reads we found in the database if we found our remote API
                    if let Some(api) = upload_api {
                        if to_upload.len() > 0 {
                            let (modified_reads, _) = remote_util::upload_all_reads(&http_client, &api, to_upload);
                            if let Ok(mut sq) = sqlite.lock() {
                                match sq.update_reads_status(&modified_reads) {
                                    Ok(_) => {},
                                    Err(e) => {
                                        println!("Error updating uploaded reads: {e}");
                                    }
                                }
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
                            }
                            if let Ok(sq) = sqlite.lock() {
                                match sq.set_setting(&Setting::new(String::from(SETTING_AUTO_REMOTE), String::from("true"))) {
                                    Ok(_) => {
                                        if let Ok(mut control) = control.lock() {
                                            control.auto_remote = true;
                                        };
                                    },
                                    Err(e) => {
                                        println!("Error saving auto upload setting: {:?}", e);
                                        no_error = write_error(&stream, errors::Errors::ServerError { message: String::from("error saving auto upload setting") });
                                    }
                                }
                            };
                        }
                        AutoUploadQuery::Stop => {
                            if uploader.running() {
                                uploader.stop();
                            } else {
                                no_error = write_error(&stream, errors::Errors::NotRunning);
                            }
                            if let Ok(sq) = sqlite.lock() {
                                match sq.set_setting(&Setting::new(String::from(SETTING_AUTO_REMOTE), String::from("false"))) {
                                    Ok(_) => {
                                        if let Ok(mut control) = control.lock() {
                                            control.auto_remote = false;
                                        };
                                    },
                                    Err(e) => {
                                        println!("Error saving auto upload setting: {:?}", e);
                                        no_error = write_error(&stream, errors::Errors::ServerError { message: String::from("error saving auto upload setting") });
                                    }
                                }
                            };
                        }
                        AutoUploadQuery::Status => {
                            no_error = write_uploader_status(&stream, uploader.status());
                        }
                    }
                    #[cfg(target_os = "linux")]
                    if let Ok(mut screen_opt) = screen.lock() {
                        if let Some(screen) = &mut *screen_opt {
                            screen.update_settings();
                            screen.update();
                        }
                    }
                },
                requests::Request::ApiResultsEventsGet { api_id } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                for api in apis {
                                    if api.id() == api_id {
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
                requests::Request::ApiResultsEventYearsGet { api_id, event_slug } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                for api in apis {
                                    if api.id() == api_id {
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
                requests::Request::ApiResultsParticipantsGet { api_id, event_slug, event_year } => {
                    if let Ok(mut sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                for api in apis {
                                    if api.id() == api_id {
                                        if api.kind() == api::API_TYPE_CHRONOKEEP_RESULTS || api.kind() == api::API_TYPE_CHRONOKEEP_RESULTS_SELF {
                                            // try to get the participants from the API
                                            let new_parts = match get_participants(&http_client, &api, &event_slug, &event_year) {
                                                Ok(new_parts) => {
                                                    new_parts
                                                },
                                                Err(e) => {
                                                    println!("error getting participants from api: {:?}", e);
                                                    no_error = write_error(&stream, e);
                                                    break;
                                                }
                                            };
                                            let new_bibchips = match get_bibchips(&http_client, &api, &event_slug, &event_year) {
                                                Ok(new_bibchips) => {
                                                    new_bibchips
                                                },
                                                Err(e) => {
                                                    println!("error getting bibchips from api: {:?}", e);
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
                                            // first translate into participants and bibchips
                                            let mut parts: Vec<participant::Participant> = Vec::new();
                                            for p in &new_parts {
                                                parts.push(p.get_participant());
                                            }
                                            match sq.add_participants(&parts) {
                                                Ok(_) => { },
                                                Err(e) => {
                                                    println!("error adding participants: {e}");
                                                    no_error = write_error(&stream, errors::Errors::DatabaseError {
                                                        message: format!("error adding participants: {e}")
                                                    });
                                                    break;
                                                },
                                            }
                                            match sq.add_bibchips(&new_bibchips) {
                                                Ok(_) => { },
                                                Err(e) => {
                                                    println!("error adding bibchips: {e}");
                                                    no_error = write_error(&stream, errors::Errors::DatabaseError {
                                                        message: format!("error adding bibchips: {e}")
                                                    });
                                                    break;
                                                }
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
                },
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
                        let mut parts: Vec<participant::Participant> = Vec::new();
                        for p in participants {
                            parts.push(p.get_participant());
                        }
                        match sq.add_participants(&parts) {
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
                requests::Request::BibChipsGet => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_bibchips() {
                            Ok(bib_chips) => {
                                no_error = write_bibchips(&stream, &bib_chips);
                            },
                            Err(e) => {
                                println!("error getting bibchips from database. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting bibchips from database: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::BibChipsRemove => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_all_bibchips() {
                            Ok(num) => {
                                no_error = write_success(&stream, num);
                            },
                            Err(e) => {
                                println!("Error deleting bibchips. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error deleting bibchips: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::BibChipsAdd { bib_chips } => {
                    if let Ok(mut sq) = sqlite.lock() {
                        match sq.add_bibchips(&bib_chips) {
                            Ok(num) => {
                                no_error = write_success(&stream, num);
                            },
                            Err(e) => {
                                println!("Error adding bibchips. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error adding bibchips: {e}")
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
                requests::Request::SightingsGet { start_seconds, end_seconds } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_sightings(start_seconds, end_seconds) {
                            Ok(sightings) => {
                                match sq.get_bibchips() {
                                    Ok(bibchips) => {
                                        no_error = write_sightings(&stream, &sightings, &bibchips);
                                    },
                                    Err(e) => {
                                        println!("Error getting bibchips. {e}");
                                        no_error = write_error(&stream, errors::Errors::DatabaseError {
                                            message: format!("error getting bibchips: {e}")
                                        });
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error getting sightings. {e}");
                                no_error = write_error(&stream, errors::Errors::DatabaseError {
                                    message: format!("error getting sightings: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::SightingsGetAll => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_all_sightings() {
                            Ok(sightings) => {
                                match sq.get_bibchips() {
                                    Ok(bibchips) => {
                                        no_error = write_sightings(&stream, &sightings, &bibchips);
                                    },
                                    Err(e) => {
                                        println!("Error getting bibchips. {e}");
                                        no_error = write_error(&stream, errors::Errors::DatabaseError {
                                            message: format!("error getting bibchips: {e}")
                                        });
                                    }
                                }
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
                },
                requests::Request::TimeGet => {
                    no_error = write_time(&stream);
                },
                requests::Request::TimeSet { time } => {
                    let mut allowed = true;
                    if let Ok(readers) = readers.lock() {
                        for reader in readers.iter() {
                            if let Some(val) = reader.is_connected() {
                                if val {
                                    println!("User attempted to set the time while a reader is connected.");
                                    no_error = write_error(&stream, errors::Errors::NotAllowed { message: format!("setting time not allowed with a reader connected") });
                                    allowed = false;
                                    break;
                                }
                            }
                        }
                    }
                    if allowed {
                        match std::env::consts::OS {
                            "linux" => {
                                match std::process::Command::new("sudo").arg("date").arg(format!("--set={time}")).status() {
                                    Ok(_) => {
                                        match std::process::Command::new("sudo").arg("hwclock").arg("-w").status() {
                                            Ok(_) => {
                                                no_error = write_time(&stream)
                                            },
                                            Err(e) => {
                                                println!("error setting time: {e}");
                                                no_error = write_error(&stream, errors::Errors::ServerError { message: format!("error setting time: {e}") })
                                            }
                                        }
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
                    #[cfg(target_os = "linux")]
                    if let Ok(mut screen_opt) = screen.lock() {
                        if let Some(screen) = &mut *screen_opt {
                            screen.set_shutdown();
                            screen.update();
                        }
                    }
                },
                requests::Request::SetNoficiation { kind: notification } => {
                    if let Ok(sock) = stream.local_addr() {
                        //println!("sock found");
                        if sock.ip().is_loopback() {
                            //println!("sock is loopback");
                            let time = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
                            if let Ok(c_socks) = control_sockets.lock() {
                                println!("notifying connected sockets");
                                for sock in c_socks.iter() {
                                    if let Some(s) = sock {
                                        _ = write_notification(&s, &notification, &time);
                                    }
                                }
                            }
                            if let Ok(control) = control.lock() {
                                if control.auto_remote {
                                    if let Ok(sq) = sqlite.lock() {
                                        match sq.get_apis() {
                                            Ok(apis) => {
                                                for api in apis {
                                                    if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                                        notifier.send_api_notification(&api, notification);
                                                        break;
                                                    }
                                                }
                                            },
                                            Err(e) => {
                                                println!("Error trying to get apis: {e}");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                _ => {
                    println!("Unknown command received - line was {:?}", single_line);
                    no_error = write_error(&stream, errors::Errors::UnknownCommand)
                },
            }
            if no_error == false {
                break;
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
        if index < MAX_CONNECTED {
            repeaters[index] = false;
        }
    }
    if let Ok(mut repeaters) = sighting_repeaters.lock() {
        if index < MAX_CONNECTED {
            repeaters[index] = false;
        }
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

fn write_notification(
    stream: &TcpStream,
    notification: &notifications::APINotification,
    time: &String
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::Notification {
        kind: notification.clone(),
        time: String::from(time)
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("17/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("17/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

fn write_error(
    stream: &TcpStream,
    error: errors::Errors
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::Error{
        error,
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("1/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("1/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

fn write_time(
    stream: &TcpStream
) -> bool {
    let time = Utc::now();
    let utc = time.naive_utc();
    let local = Local.from_utc_datetime(&utc).format("%Y-%m-%d %H:%M:%S").to_string();
    let utc = utc.format("%Y-%m-%d %H:%M:%S").to_string();
    match serde_json::to_writer(stream, &responses::Responses::Time{
        local,
        utc,
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("2/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("2/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

pub(crate) fn get_settings(sqlite: &MutexGuard<sqlite::SQLite>) -> Vec<setting::Setting> {
    let setting_names = [
        super::SETTING_CHIP_TYPE,
        super::SETTING_PORTAL_NAME,
        super::SETTING_READ_WINDOW,
        super::SETTING_SIGHTING_PERIOD,
        super::SETTING_PLAY_SOUND,
        super::SETTING_VOLUME,
        super::SETTING_VOICE,
        super::SETTING_UPLOAD_INTERVAL,
        super::SETTING_NTFY_URL,
        super::SETTING_NTFY_USER,
        super::SETTING_NTFY_PASS,
        super::SETTING_NTFY_TOPIC,
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

pub(crate) fn write_settings(
    stream: &TcpStream,
    settings: &Vec<setting::Setting>
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::Settings{
        settings: settings.to_vec(),
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("3/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("3/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

fn write_all_settings(
    stream: &TcpStream,
    settings: &Vec<setting::Setting>,
    u_readers: &Vec<reader::Reader>,
    apis: &Vec<Api>,
    status: uploader::Status
) -> bool {
    let mut list: Vec<responses::Reader> = Vec::new();
    for r in u_readers.iter() {
        let mut antennas: [u8;MAX_ANTENNAS] = [0;MAX_ANTENNAS];
        if let Ok(ant) = r.antennas.lock() {
            for ix in 0..16 {
                antennas[ix] = ant[ix];
            };
        }
        list.push(responses::Reader{
            id: r.id(),
            name: String::from(r.nickname()),
            kind: String::from(r.kind()),
            ip_address: String::from(r.ip_address()),
            port: r.port(),
            reading: r.is_reading(),
            connected: r.is_connected(),
            auto_connect: r.auto_connect() == reader::AUTO_CONNECT_TRUE,
            antennas
        })
    };
    match serde_json::to_writer(stream, &responses::Responses::SettingsAll {
        settings: settings.to_vec(),
        readers: list,
        apis: apis.to_vec(),
        auto_upload: status,
        portal_version: env!("CARGO_PKG_VERSION")
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
            println!("16/ Something went wrong writing to the socket. {e}");
            return false;
        }
    }
}
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("16/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

pub fn write_reader_list(
    stream: &TcpStream,
    u_readers: &Vec<reader::Reader>
) -> bool {
    let mut list: Vec<responses::Reader> = Vec::new();
    for r in u_readers.iter() {
        let mut antennas: [u8;MAX_ANTENNAS] = [0;MAX_ANTENNAS];
        if let Ok(ant) = r.antennas.lock() {
            for ix in 0..16 {
                antennas[ix] = ant[ix];
            };
        }
        list.push(responses::Reader{
            id: r.id(),
            name: String::from(r.nickname()),
            kind: String::from(r.kind()),
            ip_address: String::from(r.ip_address()),
            port: r.port(),
            reading: r.is_reading(),
            connected: r.is_connected(),
            auto_connect: r.auto_connect() == reader::AUTO_CONNECT_TRUE,
            antennas
        })
    };
    match serde_json::to_writer(stream, &responses::Responses::Readers{
        readers: list,
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
            println!("4/ Something went wrong writing to the socket. {e}");
            return false;
        }
    }
}
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("4/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

fn write_api_list(
    stream: &TcpStream,
    apis: &Vec<api::Api>
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::ApiList{
        apis: apis.to_vec()
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("5/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("5/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

pub fn write_reader_antennas(
    stream: &TcpStream,
    reader_name: String,
    antennas: &[u8;MAX_ANTENNAS]
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::ReaderAntennas{
        reader_name,
        antennas: antennas.clone()
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("16/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("13/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

pub fn write_reads(
    stream: &TcpStream,
    reads: &Vec<read::Read>
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::Reads{
        list: reads.to_vec(),
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("6/ Something went wrong writing to the socket. {e}");
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("6/ Something went wrong writing to the socket. {e}");
                }
            }
        }
    };
    true
}

pub fn write_sightings(
    stream: &TcpStream,
    sightings: &Vec<sighting::Sighting>,
    bibchips: &Vec<bibchip::BibChip>
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::Sightings {
        list: sightings.to_vec(),
        bib_chips: bibchips.to_vec()
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("14/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("14/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

fn write_success(
    stream: &TcpStream,
    count: usize
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::Success {
        count
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("7/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("7/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

fn write_bibchips(
    stream: &TcpStream,
    bibchips: &Vec<bibchip::BibChip>
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::BibChips {
        bib_chips: bibchips.to_vec(),
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("16/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("16/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

fn write_participants(
    stream: &TcpStream,
    parts: &Vec<participant::Participant>
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::Participants {
        participants: parts.to_vec(),
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("8/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("8/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

fn write_connection_successful(
    stream: &TcpStream,
    name: String,
    reads: bool,
    sightings: bool,
    u_readers: &Vec<reader::Reader>,
    uploader: &Arc<Uploader>
) -> bool {
    let mut list: Vec<responses::Reader> = Vec::new();
    for r in u_readers.iter() {
        let mut antennas: [u8;MAX_ANTENNAS] = [0;MAX_ANTENNAS];
        if let Ok(ant) = r.antennas.lock() {
            for ix in 0..16 {
                antennas[ix] = ant[ix];
            };
        }
        list.push(responses::Reader{
            id: r.id(),
            name: String::from(r.nickname()),
            kind: String::from(r.kind()),
            ip_address: String::from(r.ip_address()),
            port: r.port(),
            reading: r.is_reading(),
            connected: r.is_connected(),
            auto_connect: r.auto_connect() == reader::AUTO_CONNECT_TRUE,
            antennas,
        })
    };
    let mut updatable: bool = false;
    if let Ok(env) = env::var(UPDATE_SCRIPT_ENV) {
        if env.len() > 0 {
            updatable = true;
        }
    }
    match serde_json::to_writer(stream, &responses::Responses::ConnectionSuccessful{
        name,
        kind: String::from(CONNECTION_TYPE),
        version: CONNECTION_VERS,
        reads_subscribed: reads,
        sightings_subscribed: sightings,
        readers: list,
        updatable: updatable,
        auto_upload: uploader.status(),
        portal_version: env!("CARGO_PKG_VERSION"),
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("9/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("9/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

pub fn write_keepalive(
    stream: &TcpStream
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::Keepalive) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("10/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("10/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

pub fn write_disconnect(
    stream: &TcpStream
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::Disconnect) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("11/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("11/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

pub fn write_event_list(
    stream: &TcpStream,
    events: Vec<Event>
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::Events {
        events
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("12/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("12/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

pub fn write_event_years(
    stream: &TcpStream,
    years: Vec<String>
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::EventYears { years }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("13/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("13/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

pub fn write_uploader_status(
    stream: &TcpStream,
    status: uploader::Status
) -> bool {
    match serde_json::to_writer(stream, &responses::Responses::ReadAutoUpload {
        status
    }) {
        Ok(_) => {},
        Err(e) => {
            match e.io_error_kind() {
                Some(ErrorKind::BrokenPipe) |
                Some(ErrorKind::ConnectionReset) |
                Some(ErrorKind::ConnectionAborted) => {
                    return false;
                },
                _ => {
                    println!("15/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    let mut writer = stream;
    match writer.write_all(b"\n") {
        Ok(_) => {},
        Err(e) => {
            match e.kind() {
                ErrorKind::BrokenPipe |
                ErrorKind::ConnectionReset |
                ErrorKind::ConnectionAborted => {
                    return false;
                },
                _ => {
                    println!("15/ Something went wrong writing to the socket. {e}");
                    return false;
                }
            }
        }
    };
    true
}

fn construct_headers(key: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert(AUTHORIZATION, format!("Bearer {key}").parse().unwrap());
    headers
}

pub fn upload_reads(
    http_client: &reqwest::blocking::Client,
    api: &Api,
    reads: &[read::Read]
) -> Result<usize, errors::Errors> {
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

fn get_events(
    http_client: &reqwest::blocking::Client,
    api: Api
) -> Result<Vec<Event>, errors::Errors> {
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

fn get_event_years(
    http_client: &reqwest::blocking::Client,
    api: Api,
    slug: String
) -> Result<Vec<String>, errors::Errors> {
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

fn get_participants(
    http_client: &reqwest::blocking::Client,
    api: &Api,
    slug: &str,
    year: &str
) -> Result<Vec<requests::RequestParticipant>, errors::Errors> {
    let url = api.uri();
    let response = match http_client.get(format!("{url}participants"))
        .headers(construct_headers(api.token()))
        .json(&results::requests::GetParticipantsRequest{
            slug: String::from(slug),
            year: String::from(year)
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

fn get_bibchips(
    http_client: &reqwest::blocking::Client,
    api: &Api,
    slug: &str,
    year: &str
) -> Result<Vec<bibchip::BibChip>, errors::Errors> {
    let url = api.uri();
    let response = match http_client.get(format!("{url}bibchips"))
        .headers(construct_headers(api.token()))
        .json(&results::requests::GetBibChipsRequest{
            slug: String::from(slug),
            year: String::from(year)
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
            let resp_body: results::responses::GetBibChipsResponse = match response.json() {
                Ok(it) => it,
                Err(e) => {
                    println!("error trying to parse response from api: {e}");
                    return Err(errors::Errors::ServerError { message: format!("error trying to parse response from api: {e}") })
                }
            };
            resp_body.bib_chips
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
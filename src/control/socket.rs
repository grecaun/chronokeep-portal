/*
Chronokeep Portal - RFID Tag Reading Software
Copyright (C) 2026 James Sentinella

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{env, io::{ErrorKind, Read}, net::{Shutdown, SocketAddr, TcpListener, TcpStream}, sync::{Arc, Mutex, MutexGuard}, thread::{self, JoinHandle}, time::{Duration, SystemTime, UNIX_EPOCH}};
#[cfg(target_os = "linux")]
use crate::buttons::Buttons;
#[cfg(target_os = "linux")]
use crate::battery;
#[cfg(target_os = "linux")]
use crate::{screen::CharacterDisplay};

use chrono::{DateTime, Local, TimeZone, Utc};
use reqwest::header::{HeaderMap, CONTENT_TYPE, AUTHORIZATION};
use socket2::{Socket, Type, Protocol, Domain};

use crate::{control::{client::{self, Client}, exit, SETTING_AUTO_REMOTE, SETTING_PORTAL_NAME, socket::requests::AutoUploadQuery, sound::{self, SoundType}}, database::{Database, sqlite}, network::api::{self, Api}, notifier::{self, Notifier}, objects::{read, setting::{self, Setting}}, processor, reader::{self, MAX_ANTENNAS, auto_connect, reconnector::Reconnector, zebra}, remote::{self, heartbeat::Heartbeat, remote_util, uploader::{self, Uploader, info::UploadInfo}}, sound_board::Voice};

use self::notifications::APINotification;

use super::{sound::SoundNotifier, zero_conf::ZeroConf};

pub mod requests;
pub mod responses;
pub mod errors;
pub mod notifications;

// Four for control programs such as Chronokeep desktop, plus two for the built-in web page.
pub const MAX_CONNECTED: usize = 6;
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
    keepalive: Arc<Mutex<bool>>,
    quick: bool
) {
    // Joiners are join handles for threads we spin up.
    let joiners: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    // Readers are chip readers that are saved.  They may be connected or reading as well.
    let readers: Arc<Mutex<Vec<reader::Reader>>> = Arc::new(Mutex::new(Vec::new()));

    // Control sockets are sockets that are connected and should be relayed any changes in settings / readers / apis
    // when another socket changes/deletes/adds something. -- Last spot is reserved for localhost to send shutdown command
    let control_sockets: crate::control::client::ControlSockets = Arc::new(Mutex::new(Default::default()));
    // Read repeaters are sockets that want reads to be sent to them as they're being saved.
    let read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>> = Arc::new(Mutex::new([false;MAX_CONNECTED]));
    
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

    // Serve the web page. It talks to the portal through this control socket,
    // which is already listening, so it can connect whenever it's ready.
    crate::web::start(control_port, keepalive.clone(), sqlite.clone());

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
    // Kept apart from the other threads: it's stopped last, once readers have
    // handed over their final reads.
    let read_saver = Arc::new(processor::ReadSaver::new(
        sqlite.clone(),
    ));
    let z_read_saver = read_saver.clone();
    let read_saver_thread = thread::spawn(move|| {
        z_read_saver.start();
    });

    // Start a thread to enable notifications.
    let notifier = Notifier::new(keepalive.clone(), control.clone());
    let mut t_notifier = notifier.clone();
    let n_joiner = thread::spawn(move|| {
        t_notifier.run();
    });
    if let Ok(mut j) = joiners.lock() {
        j.push(n_joiner);
    }
    
    // Create an object for tracking upload status/errors.
    let upload_info = Arc::new(Mutex::new(UploadInfo::new(uploader::Status::Unknown, 0, 0)));

    // create our reads uploader struct for auto uploading if the user wants to
    let uploader = Arc::new(uploader::Uploader::new(keepalive.clone(), sqlite.clone(), control_sockets.clone(), control.clone(), upload_info.clone()));
    let mut auto_upload = false;
    if let Ok(control) = control.lock() {
        auto_upload = control.auto_remote;
    };
    if auto_upload == true {
        println!("Starting auto upload thread.");
        let t_uploader = uploader.clone();
        let t_joiner = thread::spawn(move|| {
            t_uploader.run();
        });
        if let Ok(mut j) = joiners.lock() {
            j.push(t_joiner);
        }
    } else {
        if let Ok(mut info) = upload_info.lock() {
            info.update_status(uploader::Status::Stopped, 0);
        }
    }

    // create the auto connector for automatically connecting to readers
    let ac_state = Arc::new(Mutex::new(auto_connect::State::Unknown));
    let mut auto_connector = auto_connect::AutoConnector::new(
        ac_state.clone(),
        readers.clone(),
        joiners.clone(),
        control_sockets.clone(),
        read_repeaters.clone(),
        control.clone(),
        sqlite.clone(),
        read_saver.clone(),
        sound_notifier.clone(),
        notifier.clone(),
        uploader.clone(),
    );
    // start a thread to automatically connect to readers
    thread::spawn(move|| {
        auto_connector.run(quick);
    });

    // Check if we can start a screen
    #[cfg(target_os = "linux")]
    {
        let screen: Arc<Mutex<Option<CharacterDisplay>>> = Arc::new(Mutex::new(None));
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
                        ac_state.clone(),
                        read_saver.clone(),
                        sound_notifier.clone(),
                        joiners.clone(),
                        control_port,
                        notifier.clone(),
                        uploader.clone(),
                        upload_info.clone(),
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

    // Start our code to check the battery level.
    #[cfg(target_os = "linux")]
    let mut bat_check = battery::Checker::new(keepalive.clone(), control.clone(), notifier.clone(), control_sockets.clone(), sqlite.clone());
    #[cfg(target_os = "linux")]
    let b_joiner = thread::spawn(move|| {
        #[cfg(target_os = "linux")]
        bat_check.run();
    });
    #[cfg(target_os = "linux")]
    if let Ok(mut j) = joiners.lock() {
        j.push(b_joiner);
    }

    // Set control sockets & readers on all the readers.
    if let Ok(mut u_readers) = readers.lock() {
        for reader in u_readers.iter_mut() {
            reader.set_control_sockets(control_sockets.clone());
            reader.set_readers(readers.clone());
            reader.set_uploader(uploader.clone());
        }
    }

    // play a sound to let the user know we've booted up fully and can accept control connections
    if let Ok(control) = control.lock() {
        if control.play_sound {
            control.sound_board.play_started(control.volume);
        }
    }

    let date_time: DateTime<Local> = SystemTime::now().into();
    notifier.send_notification(notifier::Notification::Start, format!("{}", date_time.format("%Y/%m/%d %T")));

    let heartbeat: Arc<Heartbeat> = Arc::new(Heartbeat::new(control.clone(), sqlite.clone(), readers.clone(), upload_info.clone()));
    let heartbeat_thread = remote::heartbeat::spawn(heartbeat.clone());

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
                let t_sqlite = sqlite.clone();
                let t_control_sockets = control_sockets.clone();
                let t_uploader = uploader.clone();
                let t_ac_state = ac_state.clone();
                let t_sound_notifier = sound_notifier.clone();
                let t_read_saver = read_saver.clone();
                let t_notifier = notifier.clone();

                let mut placed = MAX_CONNECTED + 2;
                // Writes to this client go through its own writer thread (with a write timeout).
                if let Ok(c_sock) = Client::new(&stream) {
                    if let Ok(mut c_sockets) = control_sockets.lock() {
                        for i in 0..(MAX_CONNECTED + 1) {
                            if c_sockets[i].is_none() && i < MAX_CONNECTED {
                                c_sockets[i] = Some(c_sock.clone());
                                placed = i;
                                break;
                            // Index MAX_CONNECTED is reserved for the system to tell itself to stop running in case of 
                            // power failure or some other reason the system needs to shut itself off.
                            } else if i == MAX_CONNECTED && addr.ip().is_loopback() && c_sockets[MAX_CONNECTED].is_none() {
                                c_sockets[i] = Some(c_sock.clone());
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
                                c_sock,
                                t_keepalive,
                                t_control,
                                &control_port,
                                t_readers,
                                t_joiners,
                                t_read_repeaters,
                                t_control_sockets,
                                t_sqlite,
                                t_uploader,
                                t_ac_state,
                                t_read_saver,
                                t_sound_notifier,
                                t_notifier,
                            );
                        });
                        if let Ok(mut j) = joiners.lock() {
                            // Drop threads that have already finished (closed connections)
                            // so the list doesn't grow with every connection.
                            j.retain(|h| !h.is_finished());
                            j.push(l_joiner);
                        } else {
                            println!("Unable to get joiners lock.");
                        }
                    } else {
                        _ = write_error(&c_sock, errors::Errors::TooManyConnections);
                        c_sock.end();
                    }
                } else {
                    println!("Unable to set up writing to the new connection.");
                }
            },
            Err(e) => {
                if e.kind() != ErrorKind::WouldBlock {
                    println!("Connection failed. {e}")
                }
            }
        }
    }
    println!("Shutting down control thread.");
    // Queue the shutdown notifications first so the notifier sends them before it stops.
    let date_time: DateTime<Local> = SystemTime::now().into();
    notifier.send_notification(notifier::Notification::Shutdown, format!("{}", date_time.format("%Y/%m/%d %T")));
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
    // Stop heartbeats so none go out while shutting down. This waits for at
    // most one request already in progress.
    heartbeat.stop();
    _ = heartbeat_thread.join();
    if uploader.running() {
        uploader.stop();
    }
    println!("Stopping readers.");
    if let Ok(mut r) = readers.lock() {
        for reader in r.iter_mut() {
            _ = reader.disconnect();
        }
    }
    // Reader threads save their remaining reads as they exit; the other threads
    // (uploader, notifier, ...) stop on their own now that keepalive is off.
    println!("Waiting for threads to stop.");
    if !join_all(&joiners, THREAD_STOP_WAIT) {
        println!("Some threads didn't stop within {} seconds; continuing shutdown.", THREAD_STOP_WAIT.as_secs());
    }
    // Readers are done, so everything they handed over is queued; save it.
    read_saver.stop();
    if !join_all(&Arc::new(Mutex::new(vec![read_saver_thread])), THREAD_STOP_WAIT) {
        println!("Read saver didn't stop within {} seconds.", THREAD_STOP_WAIT.as_secs());
    }
    // Tell clients we're stopping, then give their writer threads a moment to
    // send that before the connections close.
    let clients = client::connected(&control_sockets);
    for (_, sock) in &clients {
        _ = write_notification(sock, &APINotification::ShuttingDown, &format!("now"));
        _ = write_disconnect(sock);
        sock.end();
    }
    for (_, sock) in &clients {
        sock.finish(CLIENT_FINISH_WAIT);
    }
    println!("Finished control thread shutdown.");
}

/// Sends reads to every client that asked for new reads. The clients are copied
/// out first, so no lock is held while sending; a client that can't take them
/// is disconnected and unsubscribed. Returns false if any client failed.
pub fn broadcast_reads(
    control_sockets: &client::ControlSockets,
    read_repeaters: &Arc<Mutex<[bool;MAX_CONNECTED]>>,
    reads: &Vec<read::Read>,
) -> bool {
    let wanted: [bool; MAX_CONNECTED] = match read_repeaters.lock() {
        Ok(r) => *r,
        Err(_) => return false,
    };
    let mut ok = true;
    for (ix, sock) in client::connected(control_sockets) {
        if ix < MAX_CONNECTED && wanted[ix] && !write_reads(&sock, reads) {
            ok = false;
            sock.close();
            if let Ok(mut r) = read_repeaters.lock() {
                r[ix] = false;
            }
        }
    }
    ok
}

/// How long shutdown waits for each client to be sent its last messages.
pub(crate) const CLIENT_FINISH_WAIT: Duration = Duration::from_secs(1);

/// How long shutdown waits for threads to stop before carrying on.
pub(crate) const THREAD_STOP_WAIT: Duration = Duration::from_secs(15);

/// Joins every thread in `joiners` (including any added while waiting), giving
/// up after `limit`. Returns true if they all finished. A helper thread does
/// the joining so the wait is a single timed receive, not a polling loop.
pub(crate) fn join_all(joiners: &Arc<Mutex<Vec<JoinHandle<()>>>>, limit: Duration) -> bool {
    let joiners = joiners.clone();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        loop {
            let batch = match joiners.lock() {
                Ok(mut j) => std::mem::take(&mut *j),
                Err(_) => break,
            };
            if batch.is_empty() {
                break;
            }
            for handle in batch {
                _ = handle.join();
            }
        }
        _ = done_tx.send(());
    });
    done_rx.recv_timeout(limit).is_ok()
}

/// Parses one message from a control connection. Anything before the opening
/// brace (stray bytes, leftovers from a garbled message) is ignored so it
/// doesn't take the request with it.
fn parse_request(message: &str) -> Result<requests::Request, serde_json::Error> {
    let start = message.find(JSON_START_CHAR).unwrap_or(0);
    serde_json::from_str(&message[start..])
}

/// Adds bytes read from a control connection to the text buffer. A character
/// split across two reads (e.g. the "é" in "Café") is held in `pending` until
/// the rest arrives; bytes that can never be valid UTF-8 become U+FFFD.
fn push_received(pending: &mut Vec<u8>, buffer: &mut String, data: &[u8]) {
    pending.extend_from_slice(data);
    loop {
        match std::str::from_utf8(pending) {
            Ok(s) => {
                buffer.push_str(s);
                pending.clear();
                return;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                // Safe: everything before valid_up_to is valid UTF-8.
                buffer.push_str(std::str::from_utf8(&pending[..valid]).unwrap_or_default());
                match e.error_len() {
                    // Invalid bytes: replace them and keep going.
                    Some(bad) => {
                        println!("Invalid text received on a control connection.");
                        buffer.push(char::REPLACEMENT_CHARACTER);
                        pending.drain(..valid + bad);
                    }
                    // The end is a partial character: wait for the rest.
                    None => {
                        pending.drain(..valid);
                        return;
                    }
                }
            }
        }
    }
}

/// Reads to send for a manual upload: those with a read time (Unix seconds)
/// between `start` and `end` inclusive, either of which may be left open, and
/// from `reader_name` only (ignoring case) if one is given.
pub(crate) fn reads_for_manual_upload(
    sq: &sqlite::SQLite,
    start: Option<i64>,
    end: Option<i64>,
    reader_name: Option<&str>,
) -> Result<Vec<read::Read>, crate::database::DBError> {
    let mut reads = match (start, end) {
        (None, None) => sq.get_all_reads()?,
        (start, end) => sq.get_reads(start.unwrap_or(i64::MIN), end.unwrap_or(i64::MAX))?,
    };
    if let Some(name) = reader_name.map(str::trim).filter(|n| !n.is_empty()) {
        reads.retain(|r| r.reader().eq_ignore_ascii_case(name));
    }
    Ok(reads)
}

/// Finds the index of the last byte of the first JSON object in `buffer` (or of
/// the newline right after it). Returns the last index if the object isn't complete.
/// Braces inside strings (names, API keys, passwords) don't count.
fn find_json_end(buffer: &String) -> usize {
    let chars = buffer.as_bytes();
    let mut depth: usize = 0;
    let mut started = false;
    let mut in_string = false;
    let mut escaped = false;
    for (ix, &c) in chars.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        if !started {
            // get to the first start char
            if c == JSON_START_CHAR as u8 {
                started = true;
                depth = 1;
            }
            continue;
        }
        if c == b'"' {
            in_string = true;
        } else if c == JSON_START_CHAR as u8 {
            depth += 1;
        } else if c == JSON_END_CHAR as u8 {
            depth -= 1;
            if depth == 0 {
                if ix + 1 < chars.len() && chars[ix + 1] == '\n' as u8 {
                    return ix + 1;
                }
                return ix;
            }
        }
    }
    chars.len().saturating_sub(1)
}

fn handle_stream(
    index: usize,
    mut stream: TcpStream,
    client: Client,
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<super::Control>>,
    control_port: &u16,
    readers: Arc<Mutex<Vec<reader::Reader>>>,
    joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
    read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
    control_sockets: crate::control::client::ControlSockets,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    uploader: Arc<uploader::Uploader>,
    ac_state: Arc<Mutex<auto_connect::State>>,
    read_saver: Arc<processor::ReadSaver>,
    sound: Arc<SoundNotifier>,
    notifier: notifier::Notifier
) {
    println!("Starting control loop for index {index}");
    let mut data = [0 as u8; 51200];
    let mut buffer = String::new();
    // Bytes of a character split across two reads, waiting for the rest.
    let mut pending_bytes: Vec<u8> = Vec::new();
    let mut no_error = true;
    let mut client_disconnected = false;
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
            // The client closed the connection.
            Ok(0) => break,
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
            push_received(&mut pending_bytes, &mut buffer, &data[0..size]);
        }
        while buffer.len() > 0
        {
            // custom parse //
            let mut newline = find_json_end(&buffer);
            if newline < buffer.len() {
                newline += 1;
            }
            let single_line: String = buffer.drain(..newline).collect();
            let cmd: requests::Request = match parse_request(&single_line) {
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
                    // break the loop to exit the thread
                    client_disconnected = true;
                    break;
                },
                requests::Request::Connect { reads } => {
                    let mut name = String::from("Unknown");
                    if let Ok(sq) = sqlite.lock() {
                        if let Ok(set) = sq.get_setting(SETTING_PORTAL_NAME) {
                            name = String::from(set.value())
                        }
                    }
                    // The reserved local slot (index MAX_CONNECTED) has no read repeater.
                    if let Ok(mut repeaters) = read_repeaters.lock() {
                        if index < MAX_CONNECTED {
                            repeaters[index] = reads;
                        }
                    }
                    // Waits for the readers lock; with try_lock a busy lock refused the connection.
                    if let Ok(u_readers) = readers.lock() {
                        no_error = write_connection_successful(&client, name, reads, &*u_readers, &uploader);
                    } else {
                        no_error = write_error(&client, errors::Errors::ServerError { message: String::from("unable to get readers mutex") })
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
                                    no_error = write_reader_list(&client, &*u_readers);
                                }
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&client, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&client, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderAdd { id, name, kind, ip_address, port, auto_connect } => {
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
                                if let Ok(mut sq) = sqlite.lock() {
                                    let mut loc_ac = reader::AUTO_CONNECT_FALSE;
                                    if auto_connect == true {
                                        loc_ac = reader::AUTO_CONNECT_TRUE
                                    }
                                    match reader::Reader::new_no_repeaters(
                                        id,
                                        kind,
                                        name,
                                        ip_address,
                                        port,
                                        loc_ac,
                                    ) {
                                        Ok(reader) => {
                                            let port = if port < 100 {zebra::DEFAULT_ZEBRA_PORT} else {port};
                                            let mut tmp = reader;
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
                                                        let clients = crate::control::client::connected(&control_sockets);
                                                        if clients.is_empty() {
                                                            no_error = write_reader_list(&client, &*u_readers);
                                                        }
                                                        for (_, sock) in clients {
                                                            // we might be writing to other sockets
                                                            // so errors here shouldn't close our connection
                                                            _ = write_reader_list(&sock, &*u_readers);
                                                        }
                                                    }
                                                },
                                                Err(e) => {
                                                    println!("Error saving reader to database: {e}");
                                                    no_error = write_error(&client, errors::Errors::DatabaseError {
                                                        message: format!("unexpected error saving reader to database: {e}"),
                                                    });
                                                },
                                            };
                                        },
                                        Err(e) => {
                                            no_error = write_error(&client, errors::Errors::InvalidReaderType {
                                                message: e.to_string()
                                             });
                                        },
                                    }
                                }
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&client, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&client, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderRemove { id } => {
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
                                if let Ok(mut sq) = sqlite.lock() {
                                    if let Ok(mut u_readers) = readers.lock() {
                                        match sq.delete_reader(&id) {
                                            Ok(_) => {
                                                match u_readers.iter().position(|x| x.id() == id) {
                                                    Some(ix) => {
                                                        u_readers.remove(ix);
                                                        ()
                                                    },
                                                    None => {},
                                                }
                                            },
                                            Err(e) => {
                                                println!("Error removing database from reader: {e}");
                                                no_error = write_error(&client, errors::Errors::DatabaseError {
                                                    message: format!("unexpected error removing reader from database: {e}")
                                                });
                                            },
                                        }
                                        let clients = crate::control::client::connected(&control_sockets);
                                        if clients.is_empty() {
                                            no_error = write_reader_list(&client, &*u_readers);
                                        }
                                        for (_, sock) in clients {
                                            // we might be writing to other sockets
                                            // so errors here shouldn't close our connection
                                            _ = write_reader_list(&sock, &*u_readers);
                                        }
                                    }
                                }
                            },
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&client, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&client, errors::Errors::StartingUp)
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
                                                    readers.clone(),
                                                ) {
                                                    Ok(mut reader) => {
                                                        let reconnector = Reconnector::new(
                                                            readers.clone(),
                                                            joiners.clone(),
                                                            control_sockets.clone(),
                                                            read_repeaters.clone(),
                                                            control.clone(),
                                                            sqlite.clone(),
                                                            read_saver.clone(),
                                                            sound.clone(),
                                                            reader.id(),
                                                            1,
                                                            notifier.clone(),
                                                            uploader.clone(),
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
                                                                sound.notify_custom(SoundType::Connected);
                                                            },
                                                            Err(e) => {
                                                                println!("Error connecting to reader: {e}");
                                                                no_error = write_error(&client, errors::Errors::ReaderConnection {
                                                                    message: format!("error connecting to reader: {e}")
                                                                });
                                                            }
                                                        }
                                                        u_readers.push(reader);
                                                    },
                                                    Err(e) => {
                                                        u_readers.push(old_reader);
                                                        no_error = write_error(&client, errors::Errors::InvalidReaderType { message: e.to_string() });
                                                    }
                                                };
                                            } else {
                                                no_error = write_error(&client, errors::Errors::AlreadyRunning);
                                            }
                                        },
                                        None => {
                                            no_error = write_error(&client, errors::Errors::NotFound);
                                        }
                                    };
                                }
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&client, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&client, errors::Errors::StartingUp)
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
                                                    no_error = write_error(&client, errors::Errors::ReaderConnection {
                                                        message: format!("error stopping reader: {e}")
                                                    });
                                                }
                                            }
                                            match reader.disconnect() {
                                                Ok(_) => {},
                                                Err(e) => {
                                                    println!("Error connecting to reader: {e}");
                                                    no_error = write_error(&client, errors::Errors::ReaderConnection {
                                                        message: format!("error disconnecting reader: {e}")
                                                    });
                                                }
                                            }
                                            u_readers.push(reader);
                                        },
                                        None => {
                                            no_error = write_error(&client, errors::Errors::NotFound);
                                        }
                                    };
                                    thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                    let clients = crate::control::client::connected(&control_sockets);
                                    if clients.is_empty() {
                                        no_error = write_reader_list(&client, &*u_readers) && no_error;
                                    }
                                    for (_, sock) in clients {
                                        no_error = write_reader_list(&sock, &*u_readers) && no_error;
                                    }
                                }
                            }
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&client, errors::Errors::StartingUp)
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&client, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderStartAll => { // START ALL
                    if let Ok(ac) = ac_state.lock() {
                        match *ac {
                            auto_connect::State::Finished |
                            auto_connect::State::Unknown => {
                                if let Ok(mut u_readers) = readers.lock() {
                                    // make sure to iterate through the vec in reverse so we don't have some weird loop issues
                                    let num_readers = u_readers.len();
                                    let mut connected_readers: usize = 0;
                                    for ix in (0..num_readers).rev() {
                                        let mut reader = u_readers.remove(ix);
                                        if reader.is_connected() != Some(true) {
                                            reader.set_control_sockets(control_sockets.clone());
                                            reader.set_readers(readers.clone());
                                            reader.set_read_repeaters(read_repeaters.clone());
                                            reader.set_uploader(uploader.clone());
                                            let reconnector = Reconnector::new(
                                                readers.clone(),
                                                joiners.clone(),
                                                control_sockets.clone(),
                                                read_repeaters.clone(),
                                                control.clone(),
                                                sqlite.clone(),
                                                read_saver.clone(),
                                                sound.clone(),
                                                reader.id(),
                                                1,
                                                notifier.clone(),
                                                uploader.clone(),
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
                                                    connected_readers += 1;
                                                },
                                                Err(e) => {
                                                    println!("Error connecting to reader: {e}");
                                                    no_error = write_error(&client, errors::Errors::ReaderConnection {
                                                        message: format!("error connecting to reader: {e}")
                                                    });
                                                }
                                            }
                                        }
                                        u_readers.push(reader);
                                    }
                                    if num_readers == connected_readers {
                                        sound.notify_custom(SoundType::Connected);
                                    }
                                    thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                    let clients = crate::control::client::connected(&control_sockets);
                                    if clients.is_empty() {
                                        no_error = write_reader_list(&client, &*u_readers) && no_error;
                                    }
                                    for (_, sock) in clients {
                                        no_error = write_reader_list(&sock, &*u_readers) && no_error;
                                    }
                                }
                            },
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&client, errors::Errors::StartingUp);
                            }
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&client, errors::Errors::StartingUp)
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
                                                    no_error = write_error(&client, errors::Errors::ReaderConnection {
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
                                                    no_error = write_error(&client, errors::Errors::ReaderConnection {
                                                        message: format!("error discconnecting reader: {e}")
                                                    });
                                                }
                                            }
                                        }
                                        u_readers.push(reader);
                                    }
                                    thread::sleep(Duration::from_millis(CONNECTION_CHANGE_PAUSE));
                                    let clients = crate::control::client::connected(&control_sockets);
                                    if clients.is_empty() {
                                        no_error = write_reader_list(&client, &*u_readers) && no_error;
                                    }
                                    for (_, sock) in clients {
                                        no_error = write_reader_list(&sock, &*u_readers) && no_error;
                                    }
                                }
                            },
                            _ => {
                                println!("Auto connect is working right now.");
                                sound.notify_custom(SoundType::StartupInProgress);
                                no_error = write_error(&client, errors::Errors::StartingUp)
                            },
                        }
                    } else {
                        println!("Auto connect is working right now.");
                        sound.notify_custom(SoundType::StartupInProgress);
                        no_error = write_error(&client, errors::Errors::StartingUp)
                    }
                },
                requests::Request::ReaderGetAll => {
                    if let Ok(u_readers) = readers.lock() {
                        no_error = write_reader_list(&client, &*u_readers) && no_error;
                    }
                }
                requests::Request::SettingsGet => {
                    if let Ok(sq) = sqlite.lock() {
                        no_error = write_settings(&client, &get_settings(&sq));
                    }
                },
                requests::Request::SettingsGetAll => {
                    if let Ok(sq) = sqlite.lock() {
                        let settings = get_settings(&sq);
                        match sq.get_apis() {
                            Ok(apis) => {
                                if let Ok(u_readers) = readers.lock() {
                                    no_error = write_all_settings(&client, &settings, &*u_readers, &apis, uploader.status());
                                } else {
                                    no_error = write_error(&client, errors::Errors::ServerError { message: String::from("error getting the readers mutex") });
                                }
                            },
                            Err(e) => {
                                println!("error getting api list. {e}");
                                no_error = write_error(&client, errors::Errors::DatabaseError {
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
                        if let Ok(mut sq) = sqlite.lock() {
                            for setting in settings {
                                match setting.name() {
                                    super::SETTING_VOICE => {
                                        let new_voice = Voice::from_str(setting.value());
                                        if new_voice == Voice::Custom && !control.sound_board.custom_available() {
                                            println!("Custom voice selected but not available.");
                                            custom_error = true;
                                        } else {
                                            match sq.set_setting(&setting) {
                                                Ok(_) => {
                                                    if let Ok(new_control) = super::Control::new(&mut sq) {
                                                        _ = control.update(new_control);
                                                    } else {
                                                        let settings = get_settings(&sq);
                                                        no_error = write_settings(&client, &settings);
                                                    }
                                                },
                                                Err(e) => {
                                                    println!("Error saving setting. {e}");
                                                    no_error = write_error(&client, errors::Errors::DatabaseError {
                                                        message: format!("error saving setting: {e}")
                                                    });
                                                }
                                            }
                                        }
                                    },
                                    super::SETTING_NTFY_URL => {
                                        let mut t_uri = String::from(setting.value().trim());
                                        if !t_uri.is_empty() && !t_uri.ends_with("/") {
                                            t_uri = format!("{t_uri}/")
                                        }
                                        match sq.set_setting(&Setting::new(String::from(setting.name()), t_uri)) {
                                            Ok(_) => {
                                                if let Ok(new_control) = super::Control::new(&mut sq) {
                                                    _ = control.update(new_control);
                                                } else {
                                                    let settings = get_settings(&sq);
                                                    no_error = write_settings(&client, &settings);
                                                }
                                            },
                                            Err(e) => {
                                                println!("Error saving setting. {e}");
                                                no_error = write_error(&client, errors::Errors::DatabaseError {
                                                    message: format!("error saving setting: {e}")
                                                });
                                            }
                                        }
                                    },
                                    super::SETTING_CHIP_TYPE |
                                    super::SETTING_PORTAL_NAME |
                                    super::SETTING_READ_WINDOW |
                                    super::SETTING_PLAY_SOUND |
                                    super::SETTING_UPLOAD_INTERVAL |
                                    super::SETTING_VOLUME |
                                    super::SETTING_NTFY_USER |
                                    super::SETTING_NTFY_PASS |
                                    super::SETTING_NTFY_TOPIC | 
                                    super::SETTING_ENABLE_NTFY |
                                    super::SETTING_SCREEN_TYPE |
                                    super::SETTING_BEEP_IGNORE => {
                                        // Out of range beep intervals snap to the nearest end of the range.
                                        let setting = if setting.name() == super::SETTING_BEEP_IGNORE {
                                            Setting::new(String::from(setting.name()), super::clamp_beep_ignore(setting.value()).to_string())
                                        } else {
                                            setting
                                        };
                                        match sq.set_setting(&setting) {
                                            Ok(_) => {
                                                if let Ok(new_control) = super::Control::new(&mut sq) {
                                                    _ = control.update(new_control);
                                                } else {
                                                    let settings = get_settings(&sq);
                                                    no_error = write_settings(&client, &settings);
                                                }
                                            },
                                            Err(e) => {
                                                println!("Error saving setting. {e}");
                                                no_error = write_error(&client, errors::Errors::DatabaseError {
                                                    message: format!("error saving setting: {e}")
                                                });
                                            }
                                        }
                                    },
                                    other => {
                                        println!("'{other}' is not a valid setting");
                                        no_error = write_error(&client, errors::Errors::DatabaseError {
                                            message: format!("'{other}' is not a valid setting")
                                        });
                                    }
                                }
                            }
                            let settings = get_settings(&sq);
                            for (_, sock) in crate::control::client::connected(&control_sockets) {
                                // we might be writing to other sockets
                                // so errors here shouldn't close our connection
                                _ = write_settings(&sock, &settings);
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
                            no_error = write_settings(&client, &settings);
                        }
                    }
                },
                requests::Request::Quit => {
                    exit::begin(&control, &keepalive, exit::ExitAction::Quit, *control_port);
                },
                // Shutdown and restart only stop the program here; main shuts down
                // or restarts once every thread has stopped and all reads are saved.
                requests::Request::Shutdown => {
                    exit::begin(&control, &keepalive, exit::ExitAction::Shutdown, *control_port);
                },
                requests::Request::Restart => {
                    exit::begin(&control, &keepalive, exit::ExitAction::Restart, *control_port);
                },
                requests::Request::ApiList => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                no_error = write_api_list(&client, &apis);
                            },
                            Err(e) => {
                                println!("error getting api list. {e}");
                                no_error = write_error(&client, errors::Errors::DatabaseError {
                                    message: format!("error getting api list: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ApiSave { id, name, kind, uri, token, enabled, heartbeat, device_name } => {
                    match kind.as_str() {
                        api::API_TYPE_CHRONOKEEP_REMOTE |
                        api::API_TYPE_CHRONOKEEP_REMOTE_SELF => {
                            if let Ok(mut sq) = sqlite.lock() {
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
                                let enable = enabled.unwrap_or(true);
                                let heart = heartbeat.unwrap_or(false);
                                let dev = device_name.unwrap_or("".into());
                                match sq.save_api(&api::Api::new(
                                    id,
                                    name,
                                    kind,
                                    token,
                                    t_uri,
                                    enable,
                                    heart,
                                    dev
                                )) {
                                    Ok(_) => {
                                        match sq.get_apis() {
                                            Ok(apis) => {
                                                let clients = crate::control::client::connected(&control_sockets);
                                                if clients.is_empty() {
                                                    no_error = write_api_list(&client, &apis);
                                                }
                                                for (_, sock) in clients {
                                                    // we might be writing to other sockets
                                                    // so errors here shouldn't close our connection
                                                    _ = write_api_list(&sock, &apis);
                                                }
                                            },
                                            Err(e) => {
                                                println!("error getting api list. {e}");
                                                no_error = write_error(&client, errors::Errors::DatabaseError {
                                                    message: format!("error getting api list: {e}")
                                                });
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error saving api {e}");
                                        no_error = write_error(&client, errors::Errors::DatabaseError {
                                            message: format!("error saving api: {e}")
                                        });
                                    }
                                }
                            }
                        },
                        other => {
                            println!("'{other}' is not a valid api type");
                            no_error = write_error(&client, errors::Errors::InvalidApiType {
                                message: format!("'{other}' is not a valid api type")
                            });
                        }
                    }
                },
                requests::Request::ApiSaveAll { list } => {
                    if let Ok(mut sq) = sqlite.lock() {
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
                                no_error = write_error(&client, errors::Errors::DatabaseError {
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
                                    _ => {
                                        invalid_type = true;
                                    }
                                }
                            }
                            // if we found a duplicate remote, don't save and write error
                            if remote_exists {
                                println!("Remote api already exists.");
                                no_error = write_error(&client, errors::Errors::TooManyRemoteApi);
                            // if there's an invalid type, don't save and write error
                            } else if invalid_type {
                                println!("One or more invalid api types found.");
                                no_error = write_error(&client, errors::Errors::InvalidApiType { message: String::from("one or more invalid api types found") });
                            // all are saveable
                            } else {
                                let mut error_saving = false;
                                // check if we have any errors saving apis
                                for api in list {
                                    let mut t_uri = match api.kind() {
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
                                        t_uri,
                                        api.enabled(),
                                        api.heartbeat(),
                                        String::from(api.device_name())
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
                                    no_error = write_error(&client, errors::Errors::DatabaseError {
                                        message: String::from("error saving one or more apis")
                                    });
                                // otherwise send everyone connected the updated list of apis
                                } else {
                                    match sq.get_apis() {
                                        Ok(apis) => {
                                            let clients = crate::control::client::connected(&control_sockets);
                                            if clients.is_empty() {
                                                no_error = write_api_list(&client, &apis);
                                            }
                                            for (_, sock) in clients {
                                                // we might be writing to other sockets
                                                // so errors here shouldn't close our connection
                                                _ = write_api_list(&sock, &apis);
                                            }
                                        },
                                        Err(e) => {
                                            println!("error getting api list. {e}");
                                            no_error = write_error(&client, errors::Errors::DatabaseError {
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
                                    _ => {
                                        invalid_type = true;
                                    }
                                }
                            }
                            // if we found a duplicate remote, don't save and write error
                            if remote_count > 1 {
                                println!("Remote api already exists.");
                                no_error = write_error(&client, errors::Errors::TooManyRemoteApi);
                            // if there's an invalid type, don't save and write error
                            } else if invalid_type {
                                println!("One or more invalid api types found.");
                                no_error = write_error(&client, errors::Errors::InvalidApiType { message: String::from("one or more invalid api types found") });
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
                                    no_error = write_error(&client, errors::Errors::DatabaseError {
                                        message: String::from("error saving one or more apis")
                                    });
                                // otherwise send everyone connected the updated list of apis
                                } else {
                                    match sq.get_apis() {
                                        Ok(apis) => {
                                            let clients = crate::control::client::connected(&control_sockets);
                                            if clients.is_empty() {
                                                no_error = write_api_list(&client, &apis);
                                            }
                                            for (_, sock) in clients {
                                                // we might be writing to other sockets
                                                // so errors here shouldn't close our connection
                                                _ = write_api_list(&sock, &apis);
                                            }
                                        },
                                        Err(e) => {
                                            println!("error getting api list. {e}");
                                            no_error = write_error(&client, errors::Errors::DatabaseError {
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
                    if let Ok(mut sq) = sqlite.lock() {
                        match sq.delete_api(&id) {
                            Ok(_) => {
                                match sq.get_apis() {
                                    Ok(apis) => {
                                        let clients = crate::control::client::connected(&control_sockets);
                                        if clients.is_empty() {
                                            no_error = write_api_list(&client, &apis);
                                        }
                                        for (_, sock) in clients {
                                            // we might be writing to other sockets
                                            // so errors here shouldn't close our connection
                                            _ = write_api_list(&sock, &apis);
                                        }
                                    },
                                    Err(e) => {
                                        println!("error getting api list. {e}");
                                        no_error = write_error(&client, errors::Errors::DatabaseError {
                                            message: format!("error getting api list: {e}")
                                        });
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error deleting api {e}");
                                no_error = write_error(&client, errors::Errors::DatabaseError {
                                    message: format!("error deleting api: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ApiRemoteManualUpload { start_seconds, end_seconds, reader_name } => {
                    let mut to_upload: Vec<read::Read> = Vec::new();
                    let mut upload_apis: Vec<api::Api> = Vec::new();
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                let mut found = false;
                                for api in apis {
                                    if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                        found = true;
                                        upload_apis.push(api.clone());
                                    }
                                }
                                if found == false {
                                    no_error = write_error(&client, errors::Errors::NoRemoteApi);
                                } else {
                                    match reads_for_manual_upload(&sq, start_seconds, end_seconds, reader_name.as_deref()) {
                                        Ok(mut reads) => {
                                            to_upload.append(&mut reads);
                                        },
                                        Err(e) => {
                                            println!("Error geting reads to upload. {e}");
                                            no_error = write_error(&client, errors::Errors::DatabaseError { message: format!("error getting reads to upload: {e}") });
                                            upload_apis.clear();
                                        }
                                    };
                                }
                            },
                            Err(e) => {
                                println!("error getting apis: {e}");
                                no_error = write_error(&client, errors::Errors::DatabaseError {
                                    message: format!("error getting apis: {e}")
                                });
                            }
                        }
                    }
                    // upload any reads we found in the database if we found our remote API
                    let mut uploaded: usize = 0;
                    let mut failures: Vec<String> = Vec::new();
                    let http_client = match remote_util::http_client(Duration::from_secs(5)) {
                        Ok(c) => Some(c),
                        Err(e) => {
                            println!("Unable to create http client: {e}");
                            None
                        }
                    };
                    for api in &upload_apis {
                        if to_upload.len() > 0 && api.enabled() {
                            let Some(http_client) = &http_client else {
                                failures.push(format!("{} (unable to create http client)", api.nickname()));
                                continue;
                            };
                            println!("Uploading {} reads to {} at {}", to_upload.len(), api.nickname(), api.uri());
                            let (modified_reads, e_count) = remote_util::upload_all_reads(http_client, &api, to_upload.clone());
                            println!("Uploaded {} reads to {} at {}. Error count: {}", modified_reads.len(), api.nickname(), api.uri(), e_count);
                            uploaded = uploaded.max(modified_reads.len());
                            if e_count > 0 {
                                failures.push(format!("{} ({} of {} reads sent)", api.nickname(), modified_reads.len(), to_upload.len()));
                            }
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
                    // Tell the requester how it went (an error was already sent if there was no API).
                    if !upload_apis.is_empty() {
                        if !upload_apis.iter().any(|a| a.enabled()) {
                            no_error = write_error(&client, errors::Errors::NotAllowed {
                                message: String::from("every remote API is turned off")
                            });
                        } else if !failures.is_empty() {
                            no_error = write_error(&client, errors::Errors::ServerError {
                                message: format!("some reads didn't upload to {}", failures.join(", "))
                            });
                        } else {
                            no_error = write_success(&client, uploaded);
                        }
                    }
                },
                requests::Request::ApiRemoteAutoUpload { query } => {
                    match query {
                        AutoUploadQuery::Start => {
                            if uploader.running() {
                                no_error = write_error(&client, errors::Errors::AlreadyRunning);
                            } else {
                                let t_uploader = uploader.clone();
                                let t_joiner = thread::spawn(move|| {
                                    t_uploader.run();
                                });
                                if let Ok(mut j) = joiners.lock() {
                                    j.push(t_joiner);
                                }
                            }
                            // control before sqlite, the order used everywhere else, so this
                            // can't deadlock with a thread holding control and waiting on sqlite.
                            if let Ok(mut control) = control.lock() {
                                if let Ok(mut sq) = sqlite.lock() {
                                    match sq.set_setting(&Setting::new(String::from(SETTING_AUTO_REMOTE), String::from("true"))) {
                                        Ok(_) => {
                                            control.auto_remote = true;
                                        },
                                        Err(e) => {
                                            println!("Error saving auto upload setting: {:?}", e);
                                            no_error = write_error(&client, errors::Errors::ServerError { message: String::from("error saving auto upload setting") });
                                        }
                                    }
                                }
                            };
                        }
                        AutoUploadQuery::Stop => {
                            if uploader.running() {
                                uploader.stop();
                            } else {
                                no_error = write_error(&client, errors::Errors::NotRunning);
                            }
                            // control before sqlite, the order used everywhere else, so this
                            // can't deadlock with a thread holding control and waiting on sqlite.
                            if let Ok(mut control) = control.lock() {
                                if let Ok(mut sq) = sqlite.lock() {
                                    match sq.set_setting(&Setting::new(String::from(SETTING_AUTO_REMOTE), String::from("false"))) {
                                        Ok(_) => {
                                            control.auto_remote = false;
                                        },
                                        Err(e) => {
                                            println!("Error saving auto upload setting: {:?}", e);
                                            no_error = write_error(&client, errors::Errors::ServerError { message: String::from("error saving auto upload setting") });
                                        }
                                    }
                                }
                            };
                        }
                        AutoUploadQuery::Status => {
                            no_error = write_uploader_status(&client, uploader.status());
                        }
                    }
                },
                requests::Request::ReadsAdd { read } => {
                    if read.is_valid() == false {
                        no_error = write_error(&client, errors::Errors::InvalidRead)
                    } else {
                        let reads: Vec<read::Read> = vec![read];
                        let mut saved = false;
                        if let Ok(mut sq) = sqlite.lock() {
                            match sq.save_reads(&reads) {
                                Ok(_) => {
                                    saved = true;
                                },
                                Err(e) => {
                                    println!("Error saving manual read: {e}");
                                    no_error = write_error(&client, errors::Errors::DatabaseError {
                                        message: format!("error saving manual read: {e}")
                                    });
                                }
                            }
                        }
                        // Sent once the database is unlocked. Failures are other clients'
                        // problems, so they don't end this client's connection.
                        if saved {
                            _ = broadcast_reads(&control_sockets, &read_repeaters, &reads);
                        }
                    }
                },
                requests::Request::ReadsGet { start_seconds, end_seconds } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_reads(start_seconds, end_seconds) {
                            Ok(reads) => {
                                no_error = write_reads(&client, &reads);
                            },
                            Err(e) => {
                                println!("Error getting reads. {e}");
                                no_error = write_error(&client, errors::Errors::DatabaseError {
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
                                no_error = write_reads(&client, &reads);
                            },
                            Err(e) => {
                                println!("Error getting reads. {e}");
                                no_error = write_error(&client, errors::Errors::DatabaseError {
                                    message: format!("error getting reads: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ReadsDelete { start_seconds, end_seconds } => {
                    if let Ok(mut sq) = sqlite.lock() {
                        match sq.delete_reads(start_seconds, end_seconds) {
                            Ok(count) => {
                                no_error = write_success(&client, count);
                            },
                            Err(e) => {
                                println!("Error deleting reads. {e}");
                                no_error = write_error(&client, errors::Errors::DatabaseError {
                                    message: format!("error deleting reads: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::ReadsDeleteAll => {
                    if let Ok(mut sq) = sqlite.lock() {
                        match sq.delete_all_reads() {
                            Ok(count) => {
                                no_error = write_success(&client, count);
                            },
                            Err(e) => {
                                println!("Error deleting reads. {e}");
                                no_error = write_error(&client, errors::Errors::DatabaseError {
                                    message: format!("error deleting reads: {e}")
                                });
                            }
                        }
                    }
                },
                requests::Request::TimeGet => {
                    no_error = write_time(&client);
                },
                requests::Request::TimeSet { time } => {
                    match std::env::consts::OS {
                        "linux" => {
                            match std::process::Command::new("sudo").arg("date").arg(format!("--set={time}")).status() {
                                Ok(_) => {
                                    match std::process::Command::new("sudo").arg("hwclock").arg("-w").status() {
                                        Ok(_) => {
                                            // Update last_received_at so the socket doesn't auto close if we jump to the future.
                                            last_received_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                                            no_error = write_time(&client)
                                        },
                                        Err(e) => {
                                            println!("error setting time: {e}");
                                            no_error = write_error(&client, errors::Errors::ServerError { message: format!("error setting time: {e}") })
                                        }
                                    }
                                },
                                Err(e) => {
                                    println!("error setting time: {e}");
                                    no_error = write_error(&client, errors::Errors::ServerError { message: format!("error setting time: {e}") })
                                }
                            }
                        },
                        other => {
                            println!("not supported on this platform ({other})");
                            no_error = write_error(&client, errors::Errors::ServerError { message: format!("not supported on this platform ({other})") })
                        }
                    }
                },
                requests::Request::Subscribe { reads } => {
                    let mut message:String = String::from("");
                    if let Ok(mut repeaters) = read_repeaters.lock() {
                        if index >= MAX_CONNECTED {
                            message = String::from("reads can't be sent to this connection")
                        } else if (repeaters[index] == true && reads == true)
                        || (repeaters[index] == false && reads == false) {
                            message = format!("reads already set to {reads}")
                        } else {
                            repeaters[index] = reads
                        }
                    }
                    if message.len() > 0 {
                        no_error = write_error(&client, errors::Errors::AlreadySubscribed { message: message });
                    }
                },
                requests::Request::Update => {
                    // Checked now so the caller hears about problems while portal is
                    // still running. The script itself runs from main once every thread
                    // has stopped and all reads are saved.
                    match exit::check(exit::ExitAction::Update, std::env::consts::OS, env::var(UPDATE_SCRIPT_ENV).ok()) {
                        Ok(()) => {
                            no_error = write_success(&client, 0);
                            exit::begin(&control, &keepalive, exit::ExitAction::Update, *control_port);
                        },
                        Err(e) => {
                            println!("error updating: {e}");
                            no_error = write_error(&client, errors::Errors::ServerError { message: format!("error updating: {e}") })
                        }
                    }
                },
                requests::Request::SetNoficiation { kind: notification } => {
                    if let Ok(sock) = stream.local_addr() {
                        //println!("sock found");
                        if sock.ip().is_loopback() {
                            //println!("sock is loopback");
                            let time = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
                            println!("notifying connected sockets");
                            for (_, sock) in client::connected(&control_sockets) {
                                _ = write_notification(&sock, &notification, &time);
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
                    no_error = write_error(&client, errors::Errors::UnknownCommand)
                },
            }
            if no_error == false {
                break;
            }
        }
        if client_disconnected {
            break;
        }
        if let Ok(time) = SystemTime::now().duration_since(UNIX_EPOCH) {
            // if we haven't received a message in 2 x the keep alive period then we've
            // probably disconnected
            if last_received_at + (2*KEEPALIVE_INTERVAL_SECONDS) < time.as_secs() {
                // and we can exit the loop because we're definitely disconnecting
                break;
            // send a keepalive message if we haven't heard from the socket in KEEPALIVE_INTERVAL_SECONDS
            } else if last_received_at + KEEPALIVE_INTERVAL_SECONDS < time.as_secs() {
                no_error = write_keepalive(&client) && no_error;
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
    _ = write_disconnect(&client);
    // Close once the disconnect message is written.
    client.end();
    _ = stream.shutdown(Shutdown::Read);
    if let Ok(mut c_socks) = control_sockets.lock() {
        // Includes the slot kept for this computer; before, it was never freed, so
        // after one use portal-quit couldn't connect to stop portal.
        if index <= MAX_CONNECTED {
            c_socks[index] = None;
        }
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

pub fn write_notification(
    stream: &Client,
    notification: &notifications::APINotification,
    time: &String
) -> bool {
    stream.send(&responses::Responses::Notification {
        kind: notification.clone(),
        time: String::from(time)
    })
}

fn write_error(
    stream: &Client,
    error: errors::Errors
) -> bool {
    stream.send(&responses::Responses::Error{
        error,
    })
}

fn write_time(
    stream: &Client
) -> bool {
    let time = Utc::now();
    let utc = time.naive_utc();
    let local = Local.from_utc_datetime(&utc).format("%Y-%m-%d %H:%M:%S").to_string();
    let utc = utc.format("%Y-%m-%d %H:%M:%S").to_string();
    stream.send(&responses::Responses::Time{
        local,
        utc,
    })
}

pub(crate) fn get_settings(sqlite: &MutexGuard<sqlite::SQLite>) -> Vec<setting::Setting> {
    let setting_names = [
        super::SETTING_CHIP_TYPE,
        super::SETTING_PORTAL_NAME,
        super::SETTING_READ_WINDOW,
        super::SETTING_PLAY_SOUND,
        super::SETTING_VOLUME,
        super::SETTING_VOICE,
        super::SETTING_UPLOAD_INTERVAL,
        super::SETTING_NTFY_URL,
        super::SETTING_NTFY_USER,
        super::SETTING_NTFY_PASS,
        super::SETTING_NTFY_TOPIC,
        super::SETTING_ENABLE_NTFY,
        super::SETTING_SCREEN_TYPE,
        super::SETTING_BEEP_IGNORE,
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
    stream: &Client,
    settings: &Vec<setting::Setting>
) -> bool {
    stream.send(&responses::Responses::Settings{
        settings: settings.to_vec(),
    })
}

fn write_all_settings(
    stream: &Client,
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
    stream.send(&responses::Responses::SettingsAll {
        settings: settings.to_vec(),
        readers: list,
        apis: apis.to_vec(),
        auto_upload: status,
        portal_version: env!("CARGO_PKG_VERSION")
    })
}

pub fn write_reader_list(
    stream: &Client,
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
    stream.send(&responses::Responses::Readers{
        readers: list,
    })
}

fn write_api_list(
    stream: &Client,
    apis: &Vec<api::Api>
) -> bool {
    stream.send(&responses::Responses::ApiList{
        apis: apis.to_vec()
    })
}

pub fn write_reader_antennas(
    stream: &Client,
    reader_name: String,
    antennas: &[u8;MAX_ANTENNAS]
) -> bool {
    stream.send(&responses::Responses::ReaderAntennas{
        reader_name,
        antennas: antennas.clone()
    })
}

pub fn write_reads(
    stream: &Client,
    reads: &Vec<read::Read>
) -> bool {
    stream.send(&responses::Responses::Reads{
        list: reads.to_vec(),
    })
}

fn write_success(
    stream: &Client,
    count: usize
) -> bool {
    stream.send(&responses::Responses::Success {
        count
    })
}

fn write_connection_successful(
    stream: &Client,
    name: String,
    reads: bool,
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
    stream.send(&responses::Responses::ConnectionSuccessful{
        name,
        kind: String::from(CONNECTION_TYPE),
        version: CONNECTION_VERS,
        reads_subscribed: reads,
        readers: list,
        updatable: updatable,
        auto_upload: uploader.status(),
        portal_version: env!("CARGO_PKG_VERSION"),
    })
}

pub fn write_keepalive(
    stream: &Client
) -> bool {
    stream.send(&responses::Responses::Keepalive)
}

pub fn write_disconnect(
    stream: &Client
) -> bool {
    stream.send(&responses::Responses::Disconnect)
}

pub fn write_uploader_status(
    stream: &Client,
    status: uploader::Status
) -> bool {
    stream.send(&responses::Responses::ReadAutoUpload {
        status
    })
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
                return Err(errors::Errors::ServerError { message: format!("error trying to talk to api: {e}") })
            }
        };
    let output = match response.status() {
        reqwest::StatusCode::OK => {
            let resp_body: remote::responses::UploadReadsResponse = match response.json() {
                Ok(it) => it,
                Err(e) => {
                    return Err(errors::Errors::ServerError { message: format!("error trying to parse response from api: {e}") })
                }
            };
            resp_body.count
        },
        other => {
            return Err(errors::Errors::ServerError { message: format!("invalid status code: {other}") })
        }
    };
    Ok(output)
}

#[cfg(test)]
mod tests;

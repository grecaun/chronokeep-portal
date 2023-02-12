use std::{thread::{JoinHandle, self}, sync::{Mutex, Arc, MutexGuard}, net::{TcpListener, TcpStream, Shutdown}, io::Read};

use chrono::Utc;

use crate::{database::{sqlite, Database}, reader::{self, zebra, Reader}, objects::{setting, participant}, network::api};

use super::zero_conf::ZeroConf;

pub mod requests;
pub mod responses;

pub const MAX_CONNECTED: usize = 4;
pub const CONNECTION_TYPE: &str = "chrono_portal";
pub const CONNECTION_VERS: usize = 1;

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

    // run a thread for keepalive to check in on threads so we can close them
    let keep = super::keepalive::KeepAlive::new(control_sockets.clone(), keepalive.clone());
    let k_joiner = thread::spawn(move|| {
        keep.run_loop();
    });

    if let Ok(mut j) = joiners.lock() {
        j.push(z_joiner);
        j.push(k_joiner);
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
                        match write_error(&stream, String::from("too many connections")) {
                            Ok(_) => (),
                            Err(_) => {
                                println!("error writing to socket")
                                // TODO break and kill program?
                            }
                        }
                    }
                } else {
                    match write_error(&stream, String::from("unable to clone stream")) {
                        Ok(_) => (),
                        Err(_) => {
                            println!("error writing to socket")
                            // TODO break and kill program?
                        }
                    }
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
    let mut data = [0 as u8; 51200];
    match write_connection_successful(&stream) {
        Ok(_) => (),
        Err(_) => {
            println!("error writing to socket")
            // TODO break and close this connection?
        }
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
        let size = match stream.read(&mut data) {
            Ok(size) => size,
            Err(e) => {
                println!("Error reading from socket. {e}");
                stream.shutdown(Shutdown::Both).unwrap();
                break;
            },
        };
        if size > 0 {
            let cmd: requests::Request = match serde_json::from_slice(&data[0..size]) {
                Ok(data) => data,
                Err(e) => {
                    println!("Error deserializing request. {e}");
                    requests::Request::Unknown
                },
            };
            match cmd {
                requests::Request::ReaderList => {
                    if let Ok(u_readers) = readers.lock() {
                        match write_reader_list(&stream, &u_readers) {
                            Ok(_) => (),
                            Err(_) => {
                                println!("error writing to socket")
                                // TODO break and close this connection?
                            }
                        }
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
                                                        match write_reader_list(&sock, &u_readers) {
                                                            Ok(_) => (),
                                                            Err(_) => {
                                                                println!("error writing to socket")
                                                                // TODO break and close this connection?
                                                            }
                                                        }
                                                    }
                                                }
                                            } else {
                                                match write_reader_list(&stream, &u_readers) {
                                                    Ok(_) => (),
                                                    Err(_) => {
                                                        println!("error writing to socket")
                                                        // TODO break and close this connection?
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error saving reader to database: {e}");
                                        match write_error(&stream, format!("unexpected error saving reader to database: {e}")) {
                                            Ok(_) => (),
                                            Err(_) => {
                                                println!("error writing to socket")
                                                // TODO break and close this connection?
                                            }
                                        }
                                    },
                                };
                            },
                            other => {
                                match write_error(&stream, format!("'{}' is not a valid reader type. Valid Types: '{}'", other, reader::READER_KIND_ZEBRA)) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
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
                                match write_error(&stream, format!("unexpected error removing reader from database: {e}")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            },
                        }
                    }
                    if let Ok(u_readers) = readers.lock() {
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                if let Some(sock) = sock {
                                    match write_reader_list(&sock, &u_readers) {
                                        Ok(_) => (),
                                        Err(_) => {
                                            println!("error writing to socket")
                                            // TODO break and close this connection?
                                        }
                                    }
                                }
                            }
                        } else {
                            match write_reader_list(&stream, &u_readers) {
                                Ok(_) => (),
                                Err(_) => {
                                    println!("error writing to socket")
                                    // TODO break and close this connection?
                                }
                            }
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
                                                match write_error(&stream, format!("error connecting to reader: {e}")) {
                                                    Ok(_) => (),
                                                    Err(_) => {
                                                        println!("error writing to socket")
                                                        // TODO break and close this connection?
                                                    }
                                                }
                                            }
                                        }
                                        u_readers.push(Box::new(reader));
                                    },
                                    other => {
                                        match write_error(&stream, format!("'{other}' reader type not yet implemented or invalid")) {
                                            Ok(_) => (),
                                            Err(_) => {
                                                println!("error writing to socket")
                                                // TODO break and close this connection?
                                            }
                                        }
                                        u_readers.push(reader);
                                    }
                                }
                            },
                            None => {
                                match write_error(&stream, String::from("reader not found")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                if let Some(sock) = sock {
                                    match write_reader_list(&sock, &u_readers) {
                                        Ok(_) => (),
                                        Err(_) => {
                                            println!("error writing to socket")
                                            // TODO break and close this connection?
                                        }
                                    }
                                }
                            }
                        } else {
                            match write_reader_list(&stream, &u_readers) {
                                Ok(_) => (),
                                Err(_) => {
                                    println!("error writing to socket")
                                    // TODO break and close this connection?
                                }
                            }
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
                                        match write_error(&stream, format!("error connecting to reader: {e}")) {
                                            Ok(_) => (),
                                            Err(_) => {
                                                println!("error writing to socket")
                                                // TODO break and close this connection?
                                            }
                                        }
                                    }
                                }
                                u_readers.push(reader);
                            },
                            None => {
                                match write_error(&stream, String::from("reader not found")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                if let Some(sock) = sock {
                                    match write_reader_list(&sock, &u_readers) {
                                        Ok(_) => (),
                                        Err(_) => {
                                            println!("error writing to socket")
                                            // TODO break and close this connection?
                                        }
                                    }
                                }
                            }
                        } else {
                            match write_reader_list(&stream, &u_readers) {
                                Ok(_) => (),
                                Err(_) => {
                                    println!("error writing to socket")
                                    // TODO break and close this connection?
                                }
                            }
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
                                        match write_error(&stream, format!("error connecting to reader: {e}")) {
                                            Ok(_) => (),
                                            Err(_) => {
                                                println!("error writing to socket")
                                                // TODO break and close this connection?
                                            }
                                        }
                                    }
                                }
                                u_readers.push(reader);
                            },
                            None => {
                                match write_error(&stream, String::from("reader not found")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                if let Some(sock) = sock {
                                    match write_reader_list(&sock, &u_readers) {
                                        Ok(_) => (),
                                        Err(_) => {
                                            println!("error writing to socket")
                                            // TODO break and close this connection?
                                        }
                                    }
                                }
                            }
                        } else {
                            match write_reader_list(&stream, &u_readers) {
                                Ok(_) => (),
                                Err(_) => {
                                    println!("error writing to socket")
                                    // TODO break and close this connection?
                                }
                            }
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
                                        match write_error(&stream, format!("error connecting to reader: {e}")) {
                                            Ok(_) => (),
                                            Err(_) => {
                                                println!("error writing to socket")
                                                // TODO break and close this connection?
                                            }
                                        }
                                    }
                                }
                                u_readers.push(reader);
                            },
                            None => {
                                match write_error(&stream, String::from("reader not found")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                if let Some(sock) = sock {
                                    match write_reader_list(&sock, &u_readers) {
                                        Ok(_) => (),
                                        Err(_) => {
                                            println!("error writing to socket")
                                            // TODO break and close this connection?
                                        }
                                    }
                                }
                            }
                        } else {
                            match write_reader_list(&stream, &u_readers) {
                                Ok(_) => (),
                                Err(_) => {
                                    println!("error writing to socket")
                                    // TODO break and close this connection?
                                }
                            }
                        }
                    }
                },
                requests::Request::SettingsGet => {
                    if let Ok(sq) = sqlite.lock() {
                        match write_settings(&stream, &get_settings(&sq)) {
                            Ok(_) => (),
                            Err(_) => {
                                println!("error writing to socket")
                                // TODO break and close this connection?
                            }
                        }
                    }
                },
                requests::Request::SettingSet { name, value } => {
                    match name.as_str() {
                        super::SETTING_CHIP_TYPE |
                        super::SETTING_PORTAL_NAME |
                        super::SETTING_READ_WINDOW |
                        super::SETTING_SIGHTING_PERIOD => {
                            if let Ok(sq) = sqlite.lock() {
                                match sq.set_setting(&setting::Setting::new(
                                    name,
                                    value
                                )) {
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
                                                    match write_settings(&sock, &settings) {
                                                        Ok(_) => (),
                                                        Err(_) => {
                                                            println!("error writing to socket")
                                                            // TODO break and close this connection?
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            match write_settings(&stream, &settings) {
                                                Ok(_) => (),
                                                Err(_) => {
                                                    println!("error writing to socket")
                                                    // TODO break and close this connection?
                                                }
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error saving setting. {e}");
                                        match write_error(&stream, format!("error saving setting: {e}")) {
                                            Ok(_) => (),
                                            Err(_) => {
                                                println!("error writing to socket")
                                                // TODO break and close this connection?
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        other => {
                            println!("'{other}' is not a valid setting");
                            match write_error(&stream, format!("'{other}' is not a valid setting")) {
                                Ok(_) => (),
                                Err(_) => {
                                    println!("error writing to socket")
                                    // TODO break and close this connection?
                                }
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
                                match write_api_list(&stream, &apis) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            },
                            Err(e) => {
                                println!("error getting api list. {e}");
                                match write_error(&stream, format!("error getting api list: {e}")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
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
                                        if let Ok(sq) = sqlite.lock() {
                                            match sq.get_apis() {
                                                Ok(apis) => {
                                                    if let Ok(c_socks) = control_sockets.lock() {
                                                        for sock in c_socks.iter() {
                                                            if let Some(sock) = sock {
                                                                match write_api_list(&sock, &apis) {
                                                                    Ok(_) => (),
                                                                    Err(_) => {
                                                                        println!("error writing to socket")
                                                                        // TODO break and close this connection?
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        match write_api_list(&stream, &apis) {
                                                            Ok(_) => (),
                                                            Err(_) => {
                                                                println!("error writing to socket")
                                                                // TODO break and close this connection?
                                                            }
                                                        }
                                                    }
                                                },
                                                Err(e) => {
                                                    println!("error getting api list. {e}");
                                                    match write_error(&stream, format!("error getting api list: {e}")) {
                                                        Ok(_) => (),
                                                        Err(_) => {
                                                            println!("error writing to socket")
                                                            // TODO break and close this connection?
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error saving api {e}");
                                        match write_error(&stream, format!("error saving api {e}")) {
                                            Ok(_) => (),
                                            Err(_) => {
                                                println!("error writing to socket")
                                                // TODO break and close this connection?
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        other => {
                            println!("'{other}' is not a valid api type");
                            match write_error(&stream, format!("'{other}' is not a valid api type")) {
                                Ok(_) => (),
                                Err(_) => {
                                    println!("error writing to socket")
                                    // TODO break and close this connection?
                                }
                            }
                        }
                    }
                },
                requests::Request::ApiRemove { name } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_api(&name) {
                            Ok(_) => {
                                if let Ok(sq) = sqlite.lock() {
                                    match sq.get_apis() {
                                        Ok(apis) => {
                                            if let Ok(c_socks) = control_sockets.lock() {
                                                for sock in c_socks.iter() {
                                                    if let Some(sock) = sock {
                                                        match write_api_list(&sock, &apis) {
                                                            Ok(_) => (),
                                                            Err(_) => {
                                                                println!("error writing to socket")
                                                                // TODO break and close this connection?
                                                            }
                                                        }
                                                    }
                                                }
                                            } else {
                                                match write_api_list(&stream, &apis) {
                                                    Ok(_) => (),
                                                    Err(_) => {
                                                        println!("error writing to socket")
                                                        // TODO break and close this connection?
                                                    }
                                                }
                                            }
                                        },
                                        Err(e) => {
                                            println!("error getting api list. {e}");
                                            match write_error(&stream, format!("error getting api list: {e}")) {
                                                Ok(_) => (),
                                                Err(_) => {
                                                    println!("error writing to socket")
                                                    // TODO break and close this connection?
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error deleting api {e}");
                                match write_error(&stream, format!("error deleting api: {e}")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
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
                                match write_participants(&stream, &parts) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            },
                            Err(e) => {
                                println!("error getting participants from database. {e}");
                                match write_error(&stream, format!("error getting participants from database: {e}")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
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
                                                    match write_participants(&sock, &parts) {
                                                        Ok(_) => (),
                                                        Err(_) => {
                                                            println!("error writing to socket")
                                                            // TODO break and close this connection?
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            match write_participants(&stream, &parts) {
                                                Ok(_) => (),
                                                Err(_) => {
                                                    println!("error writing to socket")
                                                    // TODO break and close this connection?
                                                }
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("error getting participants. {e}");
                                        match write_error(&stream, format!("error getting participants: {e}")) {
                                            Ok(_) => (),
                                            Err(_) => {
                                                println!("error writing to socket")
                                                // TODO break and close this connection?
                                            }
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error deleting participants. {e}");
                                match write_error(&stream, format!("error deleting participants: {e}")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            }
                        }
                    }
                },
                requests::Request::ReadsGet { start_seconds, end_seconds } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_reads(start_seconds, end_seconds) {
                            Ok(reads) => {
                                let mut t_reads: Vec<responses::Read> = Vec::new();
                                for read in reads {
                                    t_reads.push(responses::Read {
                                        id: read.id(),
                                        chip: String::from(read.chip()),
                                        seconds: read.seconds(),
                                        milliseconds: read.milliseconds(),
                                        antenna: read.antenna(),
                                        reader: String::from(read.reader()),
                                        rssi: String::from(read.rssi())
                                    });
                                }
                                match write_reads(&stream, &t_reads) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error getting reads. {e}");
                                match write_error(&stream, format!("error getting reads: {e}")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            }
                        }
                    }
                },
                requests::Request::ReadsGetAll => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_all_reads() {
                            Ok(reads) => {
                                let mut t_reads: Vec<responses::Read> = Vec::new();
                                for read in reads {
                                    t_reads.push(responses::Read {
                                        id: read.id(),
                                        chip: String::from(read.chip()),
                                        seconds: read.seconds(),
                                        milliseconds: read.milliseconds(),
                                        antenna: read.antenna(),
                                        reader: String::from(read.reader()),
                                        rssi: String::from(read.rssi())
                                    });
                                }
                                match write_reads(&stream, &t_reads) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error getting reads. {e}");
                                match write_error(&stream, format!("error getting reads: {e}")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            }
                        }
                    }
                },
                requests::Request::ReadsDelete { start_seconds, end_seconds } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_reads(start_seconds, end_seconds) {
                            Ok(count) => {
                                match write_success(&stream, count) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error deleting reads. {e}");
                                match write_error(&stream, format!("error deleting reads: {e}")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            }
                        }
                    }
                },
                requests::Request::ReadsDeleteAll => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_all_reads() {
                            Ok(count) => {
                                match write_success(&stream, count) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error deleting reads. {e}");
                                match write_error(&stream, format!("error deleting reads: {e}")) {
                                    Ok(_) => (),
                                    Err(_) => {
                                        println!("error writing to socket")
                                        // TODO break and close this connection?
                                    }
                                }
                            }
                        }
                    }
                },
                requests::Request::TimeGet => {
                    match write_time(&stream) {
                        Ok(_) => (),
                        Err(_) => {
                            println!("error writing to socket")
                            // TODO break and close this connection?
                        }
                    }
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
                        match write_error(&stream, message) {
                            Ok(_) => (),
                            Err(_) => {
                                println!("error writing to socket")
                                // TODO break and close this connection?
                            }
                        }
                    }
                },
                _ => {},
            }
        }
    }
}

fn get_available_port() -> u16 {
    match (4488..5588).find(|port| {
        match TcpListener::bind(("127.0.0.1", *port)) {
            Ok(_) => true,
            Err(_) => false,
        }
    }) {
        Some(port) => port,
        None => 0
    }
}

fn write_error(stream: &TcpStream, message: String) -> Result<(), &'static str> {
    match serde_json::to_writer(stream, &responses::Responses::Error{
        message,
    }) {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("1/ Something went wrong writing to socket. {e}");
            Err("error writing to socket")
        }
    }
}

fn write_time(stream: &TcpStream) -> Result<(), &'static str> {
    let time = Utc::now();
    let utc = time.naive_utc();
    let local = time.naive_local();
    match serde_json::to_writer(stream, &responses::Responses::Time{
        local: local.format("%Y-%m-%d %H:%M:%S").to_string(),
        utc: utc.format("%Y-%m-%d %H:%M:%S").to_string(),
    }) {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("2/ Something went wrong writing to socket. {e}");
            Err("error writing to socket")
        }
    }
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

fn write_settings(stream: &TcpStream, settings: &Vec<setting::Setting>) -> Result<(), &'static str> {
    match serde_json::to_writer(stream, &responses::Responses::Settings{
        settings: settings.to_vec(),
    }) {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("3/ Something went wrong writing to socket. {e}");
            Err("error writing to socket")
        }
    }
}

fn write_reader_list(stream: &TcpStream, u_readers: &MutexGuard<Vec<Box<dyn reader::Reader>>>) -> Result<(), &'static str> {
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
    match serde_json::to_writer(stream, &responses::Responses::Readers{
        readers: list,
    }) {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("4/ Something went wrong writing to socket. {e}");
            Err("error writing to socket")
        }
    }
}

fn write_api_list(stream: &TcpStream, apis: &Vec<api::Api>) -> Result<(), &'static str> {
    match serde_json::to_writer(stream, &responses::Responses::ApiList{
        apis: apis.to_vec()
    }) {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("5/ Something went wrong writing to socket. {e}");
            Err("error writing to socket")
        }
    }
}

fn write_reads(stream: &TcpStream, reads: &Vec<responses::Read>) -> Result<(), &'static str> {
    match serde_json::to_writer(stream, &responses::Responses::Reads{
        list: reads.to_vec(),
    }) {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("6/ Something went wrong writing to socket. {e}");
            Err("error writing to socket")
        }
    }
}

fn write_success(stream: &TcpStream, count: usize) -> Result<(), &'static str> {
    match serde_json::to_writer(stream, &responses::Responses::Success {
        count
    }) {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("7/ Something went wrong writing to socket. {e}");
            Err("error writing to socket")
        }
    }
}

fn write_participants(stream: &TcpStream, parts: &Vec<participant::Participant>) -> Result<(), &'static str> {
    match serde_json::to_writer(stream, &responses::Responses::Participants {
        participants: parts.to_vec(),
    }) {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("8/ Something went wrong writing to the socket. {e}");
            Err("error writing to socket")
        }
    }
}

fn write_connection_successful(stream: &TcpStream) -> Result<(), &'static str> {
    match serde_json::to_writer(stream, &responses::Responses::ConnectionSuccessful{
        kind: String::from(CONNECTION_TYPE),
        version: CONNECTION_VERS
    }) {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("9/ Something went wrong writing to the socket. {e}");
            Err("error writing to socket")
        }
    }
}

pub fn write_keepalive(stream: &TcpStream) -> Result<(), &'static str> {
    match serde_json::to_writer(stream, &responses::Responses::Keepalive{}) {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("10/ Something went wrong writing to the socket. {e}");
            Err("error writing to socket")
        }
    }
}
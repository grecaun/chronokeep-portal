use std::{thread::{JoinHandle, self}, sync::{Mutex, Arc, MutexGuard}, net::{TcpListener, TcpStream, Shutdown}, io::Read};

use chrono::Utc;

use crate::{database::{sqlite, Database}, reader::{self, zebra, Reader}, objects::{setting, participant}, network::api};

pub mod requests;
pub mod responses;

pub fn control_loop(sqlite: Arc<Mutex<sqlite::SQLite>>, controls: super::Control) {
    // Keepalive is the boolean that tells us if we need to keep running.
    let keepalive: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));
    // Joiners are join handles for threads we spin up.
    let joiners: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    // Readers are chip readers that are saved.  They may be connected or reading as well.
    let readers: Arc<Mutex<Vec<Box<dyn reader::Reader>>>> = Arc::new(Mutex::new(Vec::new()));
    // Read repeaters are sockets that want reads to be sent to them as they're being saved.
    let read_repeaters: Arc<Mutex<Vec<TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
    // Sighting repeaters are sockets that want sightings to be sent to them as they're being saved.
    let sighting_repeaters: Arc<Mutex<Vec<TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
    // Control sockets are sockets that are connected and should be relayed any changes in settings / readers / apis
    // when another socket changes/deletes/adds something.
    let control_sockets: Arc<Mutex<Vec<TcpStream>>> = Arc::new(Mutex::new(Vec::new()));

    let listener = match TcpListener::bind(format!("127.0.0.1:{}", controls.control_port)) {
        Ok(list) => list,
        Err(e) => {
            println!("Error opening listener. {e}");
            return
        }
    };

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
                    zero_conf_port: controls.zero_conf_port.clone(),
                    control_port: controls.control_port.clone(),
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
                if let Ok(c_sock) = stream.try_clone() {
                    if let Ok(mut c_sockets) = control_sockets.lock() {
                        c_sockets.push(c_sock);
                    }
                }
                let l_joiner = thread::spawn(move|| {
                    handle_stream(
                        t_stream,
                        t_keepalive,
                        t_controls,
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
            },
            Err(e) => {
                println!("Connection failed. {e}")
            }
        }
    }
}

fn handle_stream(
    mut stream: TcpStream,
    keepalive: Arc<Mutex<bool>>,
    mut controls: super::Control,
    readers: Arc<Mutex<Vec<Box<dyn reader::Reader>>>>,
    joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
    read_reapeaters: Arc<Mutex<Vec<TcpStream>>>,
    sighting_repeaters: Arc<Mutex<Vec<TcpStream>>>,
    control_sockets: Arc<Mutex<Vec<TcpStream>>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
) {
    let mut data = [0 as u8; 51200];
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
                        write_reader_list(&stream, &u_readers);
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
                                    port
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
                                                    write_reader_list(&sock, &u_readers);
                                                }
                                            } else {
                                                write_reader_list(&stream, &u_readers);
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error saving reader to database: {e}");
                                        write_error(&stream, format!("unexpected error saving reader to database: {e}"));
                                    },
                                };
                            },
                            other => {
                                write_error(&stream, format!("'{}' is not a valid reader type. Valid Types: '{}'", other, reader::READER_KIND_ZEBRA));
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
                                write_error(&stream, format!("unexpected error removing reader from database: {e}"));
                            },
                        }
                    }
                    if let Ok(u_readers) = readers.lock() {
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                write_reader_list(&sock, &u_readers);
                            }
                        } else {
                            write_reader_list(&stream, &u_readers);
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
                                        );
                                        match reader.connect(&sqlite, &controls) {
                                            Ok(j) => {
                                                if let Ok(mut join) = joiners.lock() {
                                                    join.push(j);
                                                }
                                            },
                                            Err(e) => {
                                                println!("Error connecting to reader: {e}");
                                                write_error(&stream, format!("error connecting to reader: {e}"));
                                            }
                                        }
                                        u_readers.push(Box::new(reader));
                                    },
                                    other => {
                                        write_error(&stream, format!("'{other}' reader type not yet implemented or invalid"));
                                        u_readers.push(reader);
                                    }
                                }
                            },
                            None => {
                                write_error(&stream, String::from("reader not found"));
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                write_reader_list(&sock, &u_readers);
                            }
                        } else {
                            write_reader_list(&stream, &u_readers);
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
                                        write_error(&stream, format!("error connecting to reader: {e}"));
                                    }
                                }
                                u_readers.push(reader);
                            },
                            None => {
                                write_error(&stream, String::from("reader not found"));
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                write_reader_list(&sock, &u_readers);
                            }
                        } else {
                            write_reader_list(&stream, &u_readers);
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
                                        write_error(&stream, format!("error connecting to reader: {e}"));
                                    }
                                }
                                u_readers.push(reader);
                            },
                            None => {
                                write_error(&stream, String::from("reader not found"));
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                write_reader_list(&sock, &u_readers);
                            }
                        } else {
                            write_reader_list(&stream, &u_readers);
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
                                        write_error(&stream, format!("error connecting to reader: {e}"));
                                    }
                                }
                                u_readers.push(reader);
                            },
                            None => {
                                write_error(&stream, String::from("reader not found"));
                            }
                        };
                        if let Ok(c_socks) = control_sockets.lock() {
                            for sock in c_socks.iter() {
                                write_reader_list(&sock, &u_readers);
                            }
                        } else {
                            write_reader_list(&stream, &u_readers);
                        }
                    }
                },
                requests::Request::SettingsGet => {
                    if let Ok(sq) = sqlite.lock() {
                        write_settings(&stream, &get_settings(&sq));
                    }
                },
                requests::Request::SettingSet { name, value } => {
                    match name.as_str() {
                        super::SETTING_CHIP_TYPE |
                        super::SETTING_CONTROL_PORT |
                        super::SETTING_PORTAL_NAME |
                        super::SETTING_READ_WINDOW |
                        super::SETTING_SIGHTING_PERIOD |
                        super::SETTING_ZERO_CONF_PORT => {
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
                                                write_settings(&sock, &settings);
                                            }
                                        } else {
                                            write_settings(&stream, &settings);
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error saving setting. {e}");
                                        write_error(&stream, format!("error saving setting: {e}"));
                                    }
                                }
                            }
                        },
                        other => {
                            println!("'{other}' is not a valid setting");
                            write_error(&stream, format!("'{other}' is not a valid setting"));
                        }
                    }
                },
                requests::Request::Quit => {
                    if let Ok(mut ka) = keepalive.lock() {
                        *ka = false;
                    }
                    // connect to ensure the spawning thread will exit the accept call
                    _ = TcpStream::connect(format!("127.0.0.1:{}", controls.control_port));
                },
                requests::Request::ApiList => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.get_apis() {
                            Ok(apis) => {
                                write_api_list(&stream, &apis);
                            },
                            Err(e) => {
                                println!("error getting api list. {e}");
                                write_error(&stream, format!("error getting api list: {e}"));
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
                                                            write_api_list(&sock, &apis);
                                                        }
                                                    } else {
                                                        write_api_list(&stream, &apis);
                                                    }
                                                },
                                                Err(e) => {
                                                    println!("error getting api list. {e}");
                                                    write_error(&stream, format!("error getting api list: {e}"));
                                                }
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("Error saving api {e}");
                                        write_error(&stream, format!("error saving api {e}"));
                                    }
                                }
                            }
                        },
                        other => {
                            println!("'{other}' is not a valid api type");
                            write_error(&stream, format!("'{other}' is not a valid api type"));
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
                                                    write_api_list(&sock, &apis);
                                                }
                                            } else {
                                                write_api_list(&stream, &apis);
                                            }
                                        },
                                        Err(e) => {
                                            println!("error getting api list. {e}");
                                            write_error(&stream, format!("error getting api list: {e}"));
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error deleting api {e}");
                                write_error(&stream, format!("error deleting api: {e}"))
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
                                write_participants(&stream, &parts);
                            },
                            Err(e) => {
                                println!("error getting participants from database. {e}");
                                write_error(&stream, format!("error getting participants from database: {e}"));
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
                                                write_participants(&sock, &parts);
                                            }
                                        } else {
                                            write_participants(&stream, &parts);
                                        }
                                    },
                                    Err(e) => {
                                        println!("error getting participants. {e}");
                                        write_error(&stream, format!("error getting participants: {e}"));
                                    }
                                }
                            },
                            Err(e) => {
                                println!("Error deleting participants. {e}");
                                write_error(&stream, format!("error deleting participants: {e}"));
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
                                write_reads(&stream, &t_reads);
                            },
                            Err(e) => {
                                println!("Error getting reads. {e}");
                                write_error(&stream, format!("error getting reads: {e}"));
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
                                write_reads(&stream, &t_reads);
                            },
                            Err(e) => {
                                println!("Error getting reads. {e}");
                                write_error(&stream, format!("error getting reads: {e}"));
                            }
                        }
                    }
                },
                requests::Request::ReadsDelete { start_seconds, end_seconds } => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_reads(start_seconds, end_seconds) {
                            Ok(count) => {
                                write_success(&stream, count);
                            },
                            Err(e) => {
                                println!("Error deleting reads. {e}");
                                write_error(&stream, format!("error deleting reads: {e}"));
                            }
                        }
                    }
                },
                requests::Request::ReadsDeleteAll => {
                    if let Ok(sq) = sqlite.lock() {
                        match sq.delete_all_reads() {
                            Ok(count) => {
                                write_success(&stream, count);
                            },
                            Err(e) => {
                                println!("Error deleting reads. {e}");
                                write_error(&stream, format!("error deleting reads: {e}"));
                            }
                        }
                    }
                },
                requests::Request::TimeGet => {
                    write_time(&stream);
                },
                /*
                requests::Request::TimeSet { time } => {
                    if on linux {
                        std::process::Command::new("COMMAND").arg("ARG").arg("ARG").spawn()
                    }
                }, */
                requests::Request::Subscribe { reads, sightings } => {
                    if reads {
                        if let Ok(mut repeaters) = read_reapeaters.lock() {
                            if let Ok(t_stream) = stream.try_clone() {
                                repeaters.push(t_stream);
                            }
                        }
                    }
                    if sightings {
                        if let Ok(mut repeaters) = sighting_repeaters.lock() {
                            if let Ok(t_stream) = stream.try_clone() {
                                repeaters.push(t_stream);
                            }
                        }
                    }
                },
                _ => {},
            }
        }
    }
}

fn write_error(stream: &TcpStream, message: String) {
    match serde_json::to_writer(stream, &responses::Error{
        message,
    }) {
        Ok(_) => {},
        Err(e) => {
            println!("Something went wrong writing to socket. {e}");
        }
    }
}

fn write_time(stream: &TcpStream) {
    let time = Utc::now();
    let utc = time.naive_utc();
    let local = time.naive_local();
    match serde_json::to_writer(stream, &responses::Time{
        local: local.format("%Y-%m-%d %H:%M:%S").to_string(),
        utc: utc.format("%Y-%m-%d %H:%M:%S").to_string(),
    }) {
        Ok(_) => (),
        Err(e) => {
            println!("Something went wrong writing to socket. {e}");
        }
    }
}

fn get_settings(sqlite: &MutexGuard<sqlite::SQLite>) -> Vec<setting::Setting> {
    let setting_names = [
        super::SETTING_CHIP_TYPE,
        super::SETTING_PORTAL_NAME,
        super::SETTING_READ_WINDOW,
        super::SETTING_SIGHTING_PERIOD,
        super::SETTING_CONTROL_PORT,
        super::SETTING_ZERO_CONF_PORT
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

fn write_settings(stream: &TcpStream, settings: &Vec<setting::Setting>) {
    match serde_json::to_writer(stream, &responses::Settings{
        settings: settings.to_vec(),
    }) {
        Ok(_) => {},
        Err(e) => {
            println!("Something went wrong writing to socket. {e}");
        }
    }
}

fn write_reader_list(stream: &TcpStream, u_readers: &MutexGuard<Vec<Box<dyn reader::Reader>>>) {
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
    match serde_json::to_writer(stream, &responses::Readers{
        readers: list,
    }) {
        Ok(_) => {},
        Err(e) => {
            println!("Something went wrong writing to socket. {e}");
        }
    }
}

fn write_api_list(stream: &TcpStream, apis: &Vec<api::Api>) {
    match serde_json::to_writer(stream, &responses::ApiList{
        apis: apis.to_vec()
    }) {
        Ok(_) => (),
        Err(e) => {
            println!("Something went wrong writing to socket. {e}");
        }
    }
}

fn write_reads(stream: &TcpStream, reads: &Vec<responses::Read>) {
    match serde_json::to_writer(stream, &responses::Reads{
        list: reads.to_vec(),
    }) {
        Ok(_) => (),
        Err(e) => {
            println!("Something went wrong writing to socket. {e}");
        }
    }
}

fn write_success(stream: &TcpStream, count: usize) {
    match serde_json::to_writer(stream, &responses::Success {
        count
    }) {
        Ok(_) => (),
        Err(e) => {
            println!("Something went wrong writing to socket. {e}");
        }
    }
}

fn write_participants(stream: &TcpStream, parts: &Vec<participant::Participant>) {
    match serde_json::to_writer(stream, &responses::Participants {
        participants: parts.to_vec(),
    }) {
        Ok(_) => (),
        Err(e) => {
            println!("Something went wrong writing to the socket. {e}");
        }
    }
}
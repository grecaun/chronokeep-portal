use core::str;
use std::{collections::HashMap, env, fs::{File, OpenOptions}, io::{ErrorKind, Read, Write}, net::{IpAddr, Shutdown, SocketAddr, TcpStream}, str::FromStr, sync::{self, Arc, Mutex}, thread::{self, JoinHandle}, time::{SystemTime, UNIX_EPOCH}};
use std::time::Duration;

use crate::{control::{self, socket::{self, MAX_CONNECTED}, sound::SoundNotifier}, database::{sqlite, Database}, defaults, llrp::{self, bit_masks::ParamTypeInfo, message_types::{self, get_message_name}, parameter_types::{self, get_llrp_custom_message_name}}, notifier, objects::read, processor, reader::ANTENNA_STATUS_NONE, types};

use super::{reconnector::Reconnector, ReaderStatus, ANTENNA_STATUS_CONNECTED, ANTENNA_STATUS_DISCONNECTED, MAX_ANTENNAS};

pub mod requests;

pub const DEFAULT_ZEBRA_PORT: u16 = 5084;
pub const BUFFER_SIZE: usize = 65536;
// FX7500 stops around 750k -> 900k tags, FX9600 stops around 5.5 million tags
pub const TAG_LIMIT: usize = 100000;

pub const WRITEABLE_FILE_PATH: &str = "PORTAL_WRITEABLE_FILE_PATH";
pub const ZEBRA_SHIFT: &str = "PORTAL_ZEBRA_SHIFT";

struct ReadData {
    tags: Vec<TagData>,
    antenna_data: bool,
    antennas: [u8;MAX_ANTENNAS],
    last_ka_received_at: u64,
    status_messages: Vec<(u16, bool)>
}

pub fn connect(
    reader: &mut super::Reader,
    sqlite: &Arc<Mutex<sqlite::SQLite>>,
    control: &Arc<Mutex<control::Control>>,
    read_saver: &Arc<processor::ReadSaver>,
    sound: Arc<SoundNotifier>,
    reconnector: Option<Reconnector>,
    notifier: notifier::Notifier,
) -> Result<JoinHandle<()>, &'static str> {
    let ip_addr = match IpAddr::from_str(&reader.ip_address) {
        Ok(addr) => addr,
        Err(e) => {
            println!("Error parsing ip address. {e}");
            return Err("error parsing reader ip address")
        }
    };
    let res = TcpStream::connect_timeout(&SocketAddr::new(ip_addr, reader.port), Duration::from_secs(1));
    match res {
        Err(_) => return Err("unable to connect"),
        Ok(mut tcp_stream) => {
            match tcp_stream.set_read_timeout(Some(Duration::from_secs(1))) {
                Ok(_) => {},
                Err(e) => println!("unexpected error setting read timeout on tcp stream: {e}")
            }
            match tcp_stream.set_write_timeout(Some(Duration::from_secs(1))) {
                Ok(_) => {},
                Err(e) => println!("unexpected error setting write timeout on tcp stream: {e}")
            }
            // Set reader status to Initial connection state.
            if let Ok(mut con) = reader.status.lock() {
                *con = ReaderStatus::ConnectingKeepalive;
            }
            // try to send connection messages
            match send_set_keepalive(&mut tcp_stream, &reader.msg_id) {
                Ok(_) => println!("Connection process started on reader {}.", reader.nickname()),
                Err(e) => return Err(e),
            };
            // copy tcp stream into the mutex
            reader.socket = match tcp_stream.try_clone() {
                Ok(stream) => sync::Mutex::new(Some(stream)),
                Err(_) => {
                    return Err("error copying stream to thread")
                }
            };
            // copy values for out thread
            let mut t_stream = tcp_stream;
            let t_mutex = reader.keepalive.clone();
            let msg_id = reader.msg_id.clone();
            let status = reader.status.clone();
            let t_reader_name = reader.nickname.clone();
            let t_sqlite = sqlite.clone();
            let t_control = control.clone();
            let t_sound = sound.clone();
            let t_antennas = reader.antennas.clone();
            let t_read_saver = read_saver.clone();
            let t_reader_status = reader.status.clone();
            let t_reader_status_retries = reader.status_retries.clone();

            let t_control_sockets = reader.control_sockets.clone();
            let t_read_repeaters = reader.read_repeaters.clone();
            let mut t_sight_processor = reader.sight_processor.clone();
            let t_reconnector = reconnector.clone();
            #[cfg(target_os = "linux")]
            let t_screen = reader.screen.clone();

            let output = thread::spawn(move|| {
                let buf: &mut [u8; BUFFER_SIZE] = &mut [0; BUFFER_SIZE];
                let leftover_buffer: &mut [u8; BUFFER_SIZE] = &mut [0; BUFFER_SIZE];
                let leftover_num: &mut usize = &mut 0;
                match t_stream.set_read_timeout(Some(Duration::from_secs(1))) {
                    Ok(_) => (),
                    Err(e) => {
                        println!("Error setting read timeout. {e}")
                    }
                }
                let mut read_map: HashMap<u128, (u128, TagData)> = HashMap::new();
                let mut count: usize = 0;
                let mut purge_count: usize = 0;
                let mut last_ka_received_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                let mut reconnect = false;
                loop {
                    /*
                        Start of reading loop
                     */
                    if let Ok(keepalive) = t_mutex.lock() {
                        // check if we've been told to quit
                        if *keepalive == false {
                            break;
                        };
                    }
                    #[cfg(target_os = "linux")]
                    let mut starting_status = ReaderStatus::Unknown;
                    #[cfg(target_os = "linux")]
                    if let Ok(stat) = t_reader_status.lock()  {
                        starting_status = stat.clone();
                    }
                    match read(&mut t_stream, buf, leftover_buffer, leftover_num, last_ka_received_at) {
                        Ok(data) => {
                            // process any status messages
                            if data.status_messages.len() > 0 {
                                let mut attempt = 0;
                                if let Ok(att) = t_reader_status_retries.lock() {
                                    attempt = *att;
                                }
                                for (msg_kind, success) in data.status_messages {
                                    match msg_kind {
                                        // SET_READER_CONFIG_RESPONSE is the proper response for:
                                        // SetKeepalive (step 1)
                                        // SetNoFilter (step 3)
                                        // SetReaderConfig (step 4)
                                        // EnableEventsAndReports (step 5)
                                        llrp::message_types::SET_READER_CONFIG_RESPONSE => {
                                            attempt += 1;
                                            if let Ok(mut stat) = t_reader_status.lock() {
                                            if success {
                                                attempt = 0;
                                                match *stat {
                                                    ReaderStatus::ConnectingKeepalive => {
                                                        *stat = ReaderStatus::ConnectingPurgeTags;
                                                        match send_purge_tags(&mut t_stream, &msg_id) {
                                                            Ok(_) => {
                                                                println!("-- Purge Tags request on connection sent.")
                                                            },
                                                            Err(e) => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                eprintln!("error sending purge tags message: {e}")
                                                            },
                                                        }
                                                    },
                                                    ReaderStatus::ConnectingSetNoFilter => {
                                                        *stat = ReaderStatus::ConnectingSetReaderConfig;
                                                        match send_set_reader_config(&mut t_stream, &msg_id) {
                                                            Ok(_) => {
                                                                println!("-- Set Reader Config request on connection sent.")
                                                            },
                                                            Err(e) => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                eprintln!("error sending set reader config message: {e}")
                                                            },
                                                        }
                                                    },
                                                    ReaderStatus::ConnectingSetReaderConfig => {
                                                        *stat = ReaderStatus::ConnectingDeleteAccessSpec;
                                                        // ENABLE_EVENTS_AND_REPORTS and GET_READER_CONFIG fail to report success from the reader
                                                        match send_enable_events_and_reports(&mut t_stream, &msg_id) {
                                                            Ok(_) => {
                                                                println!("-- Send Enable Events and Reports request on connection sent.")
                                                            },
                                                            Err(e) => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                eprintln!("error sending enable events and reports message: {e}")
                                                            },
                                                        }
                                                        match send_get_reader_config(&mut t_stream, &msg_id) {
                                                            Ok(_) => {
                                                                println!("-- Get Reader Config request on connection sent.")
                                                            },
                                                            Err(e) => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                eprintln!("error sending get reader config message: {e}")
                                                            },
                                                        }
                                                        match send_delete_access_spec(&mut t_stream, &msg_id) {
                                                            Ok(_) => {
                                                                println!("-- Delete Access Spec request on connection sent.")
                                                            },
                                                            Err(e) => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                eprintln!("error sending delete access spec message: {e}")
                                                            },
                                                        }
                                                    },
                                                    _ => {
                                                        *stat = ReaderStatus::Disconnected;
                                                        println!("unknown reader status while processing SET_READER_CONFIG_RESPONSE")
                                                    },
                                                }
                                                } else {
                                                    if attempt > 5 {
                                                        *stat = ReaderStatus::Disconnected;
                                                    } else {
                                                        match *stat {
                                                            ReaderStatus::ConnectingKeepalive => {
                                                                match send_set_keepalive(&mut t_stream, &msg_id) {
                                                                    Ok(_) => {
                                                                        println!("-- Set Keepalive request on connection sent.")
                                                                    },
                                                                    Err(e) => {
                                                                        *stat = ReaderStatus::Disconnected;
                                                                        eprintln!("error sending set keepalive message: {e}")
                                                                    },
                                                                }
                                                            },
                                                            ReaderStatus::ConnectingSetNoFilter => {
                                                                match send_set_no_filter(&mut t_stream, &msg_id) {
                                                                    Ok(_) => {
                                                                        println!("-- Set No Filter request on connection sent.")
                                                                    },
                                                                    Err(e) => {
                                                                        *stat = ReaderStatus::Disconnected;
                                                                        eprintln!("error sending set no filter message: {e}")
                                                                    },
                                                                }
                                                            },
                                                            ReaderStatus::ConnectingSetReaderConfig => {
                                                                match send_set_reader_config(&mut t_stream, &msg_id) {
                                                                    Ok(_) => {
                                                                        println!("-- Set Reader Config request on connection sent.")
                                                                    },
                                                                    Err(e) => {
                                                                        *stat = ReaderStatus::Disconnected;
                                                                        eprintln!("error sending set reader config message: {e}")
                                                                    },
                                                                }
                                                            },
                                                            _ => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                println!("unknown reader status while processing SET_READER_CONFIG_RESPONSE")
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        // CUSTOM_MESSAGE is the proper response for:
                                        // PurgeTags (step 2)
                                        llrp::message_types::CUSTOM_MESSAGE => {
                                            attempt += 1;
                                            if let Ok(mut stat) = t_reader_status.lock() {
                                                if success {
                                                    attempt = 0;
                                                    match *stat {
                                                        ReaderStatus::ConnectingPurgeTags => {
                                                            *stat = ReaderStatus::ConnectingSetNoFilter;
                                                            match send_set_no_filter(&mut t_stream, &msg_id) {
                                                                Ok(_) => {
                                                                    println!("-- Set No Filter request on connection sent.")
                                                                },
                                                                Err(e) => {
                                                                    *stat = ReaderStatus::Disconnected;
                                                                    eprintln!("error sending set no filter message: {e}")
                                                                },
                                                            }
                                                        },
                                                        ReaderStatus::Connected => {
                                                            println!("Successfully purged tags while connected.");
                                                        },
                                                        _ => {
                                                            *stat = ReaderStatus::Disconnected;
                                                            println!("unknown reader status while processing CUSTOM_MESSAGE")
                                                        }
                                                    }
                                                } else {
                                                    if attempt > 5 {
                                                        *stat = ReaderStatus::Disconnected;
                                                    } else {
                                                        match *stat {
                                                            ReaderStatus::ConnectingPurgeTags => {
                                                                match send_purge_tags(&mut t_stream, &msg_id) {
                                                                    Ok(_) => {
                                                                        println!("-- Purge Tags request on connection sent.")
                                                                    },
                                                                    Err(e) => {
                                                                        *stat = ReaderStatus::Disconnected;
                                                                        eprintln!("error sending purge tags message: {e}")
                                                                    },
                                                                }
                                                            },
                                                            ReaderStatus::Connected => {
                                                                println!("Successfully purged tags while connected.");
                                                            },
                                                            _ => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                println!("unknown reader status while processing CUSTOM_MESSAGE")
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        // DELETE_ACCESS_SPEC_RESPONSE is the proper response for:
                                        // DeleteAccessSpec (step 6)
                                        llrp::message_types::DELETE_ACCESS_SPEC_RESPONSE => {
                                            attempt += 1;
                                            if let Ok(mut stat) = t_reader_status.lock() {
                                                if success {
                                                    attempt = 0;
                                                    match *stat {
                                                        ReaderStatus::ConnectingDeleteAccessSpec => {
                                                            *stat = ReaderStatus::ConnectingDeleteRospec;
                                                            match send_delete_rospec(&mut t_stream, &msg_id) {
                                                                Ok(_) => {
                                                                    println!("-- Delete Rospec request on connection sent.")
                                                                },
                                                                Err(e) => {
                                                                    *stat = ReaderStatus::Disconnected;
                                                                    eprintln!("error sending delete rospec message: {e}")
                                                                },
                                                            }
                                                        }
                                                        _ => {
                                                            *stat = ReaderStatus::Disconnected;
                                                            println!("unknown reader status while processing DELETE_ACCESS_SPEC_RESPONSE")
                                                        }
                                                    }
                                                } else {
                                                    if attempt > 5 {
                                                        *stat = ReaderStatus::Disconnected;
                                                    } else {
                                                        match *stat {
                                                            ReaderStatus::ConnectingDeleteRospec => {
                                                                match send_delete_access_spec(&mut t_stream, &msg_id) {
                                                                    Ok(_) => {
                                                                        println!("-- Delete Access Spec request on connection sent.")
                                                                    },
                                                                    Err(e) => {
                                                                        *stat = ReaderStatus::Disconnected;
                                                                        eprintln!("error sending delete access spec message: {e}")
                                                                    },
                                                                }
                                                            }
                                                            _ => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                println!("unknown reader status while processing DELETE_ACCESS_SPEC_RESPONSE")
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        // DISABLE_ROSPEC_RESPONSE is the proper response for:
                                        // DisableRospec (step 1 of stopping)
                                        llrp::message_types::DISABLE_ROSPEC_RESPONSE => {
                                            attempt += 1;
                                            if let Ok(mut stat) = t_reader_status.lock() {
                                                if success {
                                                    attempt = 0;
                                                    match *stat {
                                                        ReaderStatus::Disconnected => {}
                                                        _ => {
                                                            *stat = ReaderStatus::Disconnected;
                                                            println!("unknown reader status while processing DISABLE_ROSPEC_RESPONSE")
                                                        }
                                                    }
                                                } else {
                                                    if attempt > 5 {
                                                        *stat = ReaderStatus::Disconnected;
                                                    } else {
                                                        match *stat {
                                                            ReaderStatus::Disconnected => {}
                                                            _ => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                println!("unknown reader status while processing DISABLE_ROSPEC_RESPONSE")
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        // DELETE_ROSPEC_RESPONSE is the proper response for:
                                        // DeleteRospec (step 7, step 2 of stopping)
                                        llrp::message_types::DELETE_ROSPEC_RESPONSE => {
                                            attempt += 1;
                                            if let Ok(mut stat) = t_reader_status.lock() {
                                                if success {
                                                    attempt = 0;
                                                    match *stat {
                                                        ReaderStatus::ConnectingDeleteRospec => {
                                                            *stat = ReaderStatus::ConnectingAddRospec;
                                                            match send_add_rospec(&mut t_stream, &msg_id) {
                                                                Ok(_) => {
                                                                    println!("-- Add Rospec request on connection sent.")
                                                                },
                                                                Err(e) => {
                                                                    *stat = ReaderStatus::Disconnected;
                                                                    eprintln!("error sending add rospec message: {e}")
                                                                },
                                                            }
                                                        },
                                                        ReaderStatus::Disconnected => {},
                                                        _ => {
                                                            *stat = ReaderStatus::Disconnected;
                                                            println!("unknown reader status while processing DELETE_ROSPEC_RESPONSE")
                                                        }
                                                    }
                                                } else {
                                                    if attempt > 5 {
                                                        *stat = ReaderStatus::Disconnected;
                                                    } else {
                                                        match *stat {
                                                            ReaderStatus::ConnectingAddRospec => {
                                                                match send_delete_rospec(&mut t_stream, &msg_id) {
                                                                    Ok(_) => {
                                                                        println!("-- Delete Rospec request on connection sent.")
                                                                    },
                                                                    Err(e) => {
                                                                        *stat = ReaderStatus::Disconnected;
                                                                        eprintln!("error sending delete rospec message: {e}")
                                                                    },
                                                                }
                                                            },
                                                            ReaderStatus::Disconnected => {},
                                                            _ => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                println!("unknown reader status while processing DELETE_ROSPEC_RESPONSE")
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        // ADD_ROSPEC_RESPONSE is the proper response for:
                                        // AddRospec (step 8)
                                        llrp::message_types::ADD_ROSPEC_RESPONSE => {
                                            attempt += 1;
                                            if let Ok(mut stat) = t_reader_status.lock() {
                                                if success {
                                                    attempt = 0;
                                                    match *stat {
                                                        ReaderStatus::ConnectingAddRospec => {
                                                            *stat = ReaderStatus::ConnectingEnableRospec;
                                                            match send_enable_rospec(&mut t_stream, &msg_id) {
                                                                Ok(_) => {
                                                                    println!("-- Enable Rospec request on connection sent.")
                                                                },
                                                                Err(e) => {
                                                                    *stat = ReaderStatus::Disconnected;
                                                                    eprintln!("error sending enable rospec message: {e}")
                                                                },
                                                            }
                                                        }
                                                        _ => {
                                                            *stat = ReaderStatus::Disconnected;
                                                            println!("unknown reader status while processing ADD_ROSPEC_RESPONSE")
                                                        }
                                                    }
                                                } else {
                                                    if attempt > 5 {
                                                        *stat = ReaderStatus::Disconnected;
                                                    } else {
                                                        match *stat {
                                                            ReaderStatus::ConnectingAddRospec => {
                                                                match send_add_rospec(&mut t_stream, &msg_id) {
                                                                    Ok(_) => {
                                                                        println!("-- Add Rospec request on connection sent.")
                                                                    },
                                                                    Err(e) => {
                                                                        *stat = ReaderStatus::Disconnected;
                                                                        eprintln!("error sending add rospec message: {e}")
                                                                    },
                                                                }
                                                            },
                                                            _ => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                println!("unknown reader status while processing ADD_ROSPEC_RESPONSE")
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        // ENABLE_ROSPEC_RESPONSE is the proper response for:
                                        // EnableRospec (step 9)
                                        llrp::message_types::ENABLE_ROSPEC_RESPONSE => {
                                            attempt += 1;
                                            if let Ok(mut stat) = t_reader_status.lock() {
                                                if success {
                                                    attempt = 0;
                                                    match *stat {
                                                        ReaderStatus::ConnectingEnableRospec => {
                                                            *stat = ReaderStatus::ConnectingStartRospec;
                                                            match send_start_rospec(&mut t_stream, &msg_id) {
                                                                Ok(_) => {
                                                                    println!("-- Start Rospec request on connection sent.")
                                                                },
                                                                Err(e) => {
                                                                    *stat = ReaderStatus::Disconnected;
                                                                    eprintln!("error sending start rospec message: {e}")
                                                                },
                                                            }
                                                        }
                                                        _ => {
                                                            *stat = ReaderStatus::Disconnected;
                                                            println!("unknown reader status while processing ENABLE_ROSPEC_RESPONSE")
                                                        }
                                                    }
                                                } else {
                                                    if attempt > 5 {
                                                        *stat = ReaderStatus::Disconnected;
                                                    } else {
                                                        match *stat {
                                                            ReaderStatus::ConnectingEnableRospec => {
                                                                match send_enable_rospec(&mut t_stream, &msg_id) {
                                                                    Ok(_) => {
                                                                        println!("-- Enable Rospec request on connection sent.")
                                                                    },
                                                                    Err(e) => {
                                                                        *stat = ReaderStatus::Disconnected;
                                                                        eprintln!("error sending enable rospec message: {e}")
                                                                    },
                                                                }
                                                            },
                                                            _ => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                println!("unknown reader status while processing ENABLE_ROSPEC_RESPONSE")
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        // START_ROSPEC_RESPONSE is the proper response for:
                                        // StartRospec (step 10)
                                        llrp::message_types::START_ROSPEC_RESPONSE => {
                                            attempt += 1;
                                            if let Ok(mut stat) = t_reader_status.lock() {
                                                if success {
                                                    attempt = 0;
                                                    match *stat {
                                                        ReaderStatus::ConnectingStartRospec => {
                                                            *stat = ReaderStatus::Connected;
                                                            // inform control sockets of reader connection change?
                                                        }
                                                        _ => {
                                                            *stat = ReaderStatus::Disconnected;
                                                            println!("unknown reader status while processing START_ROSPEC_RESPONSE")
                                                        }
                                                    }
                                                } else {
                                                    if attempt > 5 {
                                                        *stat = ReaderStatus::Disconnected;
                                                    } else {
                                                        match *stat {
                                                            ReaderStatus::ConnectingStartRospec => {
                                                                match send_start_rospec(&mut t_stream, &msg_id) {
                                                                    Ok(_) => {
                                                                        println!("-- Start Rospect request on connection sent.")
                                                                    },
                                                                    Err(e) => {
                                                                        *stat = ReaderStatus::Disconnected;
                                                                        eprintln!("error sending start rospec message: {e}")
                                                                    },
                                                                }
                                                            },
                                                            _ => {
                                                                *stat = ReaderStatus::Disconnected;
                                                                println!("unknown reader status while processing START_ROSPEC_RESPONSE")
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        _ => {
                                            println!("Error, unknown status message processed.");
                                        }
                                    }
                                }
                            }
                            // process tags if we were told there were some
                            if data.tags.len() > 0 {
                                t_sound.notify_one();
                                count += data.tags.len();
                                let mut tags = data.tags;
                                match process_tags(&mut read_map, &mut tags, &t_control, &t_read_saver, t_reader_name.as_str()) {
                                    Ok(new_reads) => {
                                        if new_reads.len() > 0 {
                                            match send_new(new_reads, &t_control_sockets, &t_read_repeaters) {
                                                Ok(_) => {},
                                                Err(e) => {
                                                    println!("error sending new reads to repeaters: {e}")
                                                }
                                            }
                                            if let Some(processor) = t_sight_processor {
                                                processor.notify();
                                                t_sight_processor = Some(processor);
                                            }
                                        }
                                    },
                                    Err(e) => println!("Error processing tags. {e}"),
                                };
                            }
                            // if antenna data exists then we can update the readers antennas
                            if data.antenna_data {
                                let mut updated = false;
                                if let Ok(mut ant) = t_antennas.lock() {
                                    for ix in 0..16 {
                                        if data.antennas[ix] != ANTENNA_STATUS_NONE {
                                            // The layout for antenna placement on our custom made boxes
                                            // makes the antenna numbers we see not correspond to the numbers
                                            // the Zebra FX9600 uses, so we need to shift the index
                                            let mut ix_shift = ix;
                                            // So check if the environment variable is set and shift the antenna numberings if so.
                                            if let Ok(env) = env::var(ZEBRA_SHIFT) {
                                                if env.len() > 0 {
                                                    match ix {
                                                        0 | 2 | 4 | 6 => { 
                                                            ix_shift = (ix / 2) + 4; // 1 => 5, 3 => 6, 5 => 7, 7 => 8
                                                        },
                                                        1 | 3 | 5 | 7 => {                  // Our layout for our system me
                                                            ix_shift = ((ix + 1) / 2) - 1;  // 2 => 1, 4 => 2, 6 => 3, 8 => 4
                                                        },
                                                        _ => {}
                                                    }
                                                }
                                            }
                                            ant[ix_shift] = data.antennas[ix];
                                        }
                                    }
                                    updated = true;
                                }
                                // send out notification that we updated the readers
                                if updated {
                                    match send_antennas(t_reader_name.as_str(), &t_antennas, &t_control_sockets) {
                                        Ok(_) => {},
                                        Err(e) => {
                                            println!("error sending antennas to control sockets: {e}")
                                        }
                                    }
                                    #[cfg(target_os = "linux")]
                                    if let Ok(screen_opt) = t_screen.lock() {
                                        if let Some(screen) = &*screen_opt {
                                            screen.update();
                                        }
                                    }
                                }
                            }
                            if last_ka_received_at < data.last_ka_received_at {
                                last_ka_received_at = data.last_ka_received_at
                            }
                            let right_now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                            if right_now - 5 > last_ka_received_at {
                                println!("no keep alive message received in the last 5 seconds");
                                reconnect = true;
                                break;
                            }
                        },
                        Err(e) => {
                            *leftover_num = 0;
                            match e.kind() {
                                ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset => {
                                    println!("connection aborted/reset");
                                    reconnect = true;
                                    notifier.send_notification(notifier::Notification::StopReading);
                                    break;
                                }
                                // TimedOut == Windows, WouldBlock == Linux
                                ErrorKind::TimedOut | ErrorKind::WouldBlock => {
                                    match process_tags(&mut read_map, &mut Vec::new(), &t_control, &t_read_saver, t_reader_name.as_str()) {
                                        Ok(new_reads) => {
                                            if new_reads.len() > 0 {
                                                match send_new(new_reads, &t_control_sockets, &t_read_repeaters) {
                                                    Ok(_) => {},
                                                    Err(e) => {
                                                        println!("error sending new reads to repeaters: {e}")
                                                    }
                                                }
                                                if let Some(processor) = t_sight_processor {
                                                    processor.notify();
                                                    t_sight_processor = Some(processor);
                                                }
                                            }
                                        },
                                        Err(e) => println!("Error processing tags. {e}"),
                                    }
                                    let right_now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                                    if right_now - 5 > last_ka_received_at {
                                        println!("no keep alive message received in the last 5 seconds");
                                        reconnect = true;
                                        notifier.send_notification(notifier::Notification::StopReading);
                                        break;
                                    }
                                },
                                _ => println!("Error reading from reader. {e}"),
                            }
                        }
                    }
                    if count > TAG_LIMIT {
                        purge_count += 1;
                        println!("Purging tags. This is purge number {purge_count}.");
                        match send_purge_tags(&mut t_stream, &msg_id) {
                            Ok(_) => {
                                count = 0;
                            },
                            Err(e) => {
                                println!("Error sending purge tag message. {e}");
                            }
                        }
                    }
                    #[cfg(target_os = "linux")]
                    if let Ok(stat) = t_reader_status.lock()  {
                        // Check if we had a valid starting status and it's changed to Disconnected/Connected
                        // Then update the screen if we did.
                        if starting_status != ReaderStatus::Unknown
                        && starting_status != *stat
                        && (*stat == ReaderStatus::Connected || *stat == ReaderStatus::Disconnected) {
                            if let Ok(mut screen_opt) = t_screen.lock() {
                                if let Some(screen) = &mut *screen_opt {
                                    screen.update();
                                }
                            }
                        }
                    }
                    /*
                        End of reading loop
                     */
                }
                stop(&mut t_stream, &status, &t_reader_name, &msg_id);
                finalize(&mut t_stream, &msg_id, &status, last_ka_received_at);
                save_reads(&mut read_map, &t_control, &t_sqlite, t_reader_name.as_str());
                if let Ok(mut con) = status.lock() {
                    *con = ReaderStatus::Disconnected;
                }
                println!("Thread reading from this reader has now closed.");
                if reconnect == true {
                    if let Some(rec) = t_reconnector {
                        rec.run();
                    }
                }
            });
            Ok(output)
        },
    }
}

fn send_delete_access_spec(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // delete all access spec
    let buf = requests::delete_access_spec(&local_id, &0);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}

fn send_delete_rospec(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // delete all rospec
    let buf = requests::delete_rospec(&local_id, &0);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}

fn send_add_rospec(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // add rospec
    let buf = requests::add_rospec(&local_id, &100);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}

fn send_enable_rospec(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // enable rospec
    let buf = requests::enable_rospec(&local_id, &100);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}

fn send_start_rospec(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // start rospec
    let buf = requests::start_rospec(&local_id, &100);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}

pub fn stop_reader(reader: &mut super::Reader) -> Result<(), &'static str> {
    if let Ok(mut r) = reader.status.lock() {
        if ReaderStatus::Connected != *r {
            return Err("not reading")
        }
        *r = ReaderStatus::Disconnected;
    } else {
        return Err("unable to check if we're actually reading")
    }
    let msg_id = reader.get_next_id();
    if let Ok(stream) = reader.socket.lock() {
        match &*stream {
            Some(s) => {
                let mut w_stream = match s.try_clone() {
                    Ok(v) => v,
                    Err(_) => return Err("unable to copy stream"),
                };
                match stop_reading(&mut w_stream, msg_id) {
                    Ok(_) => {
                        println!("No longer reading from reader {}", reader.nickname());
                    }
                    Err(e) => return Err(e),
                }
                w_stream.shutdown(Shutdown::Both).expect("stream shutdown failed");
            },
            None => {
                return Err("not connected")
            }
        }
        Ok(())
    } else {
        Err("unable to get stream mutex")
    }
}

fn stop(
    socket: &mut TcpStream,
    status: &Arc<Mutex<ReaderStatus>>,
    nickname: &String,
    msg_mtx: &Arc<sync::Mutex<u32>>
) {
    if let Ok(mut r) = status.lock() {
        if ReaderStatus::Disconnected == *r {
            return
        }
        *r = ReaderStatus::Disconnected;
    }
    let mut msg_id = 0;
    if let Ok(id) = msg_mtx.lock() {
        msg_id = *id+1;
    }
    match stop_reading(socket, msg_id) {
        Ok(_) => println!("No longer reading from reader {}", nickname),
        Err(_) => (),
    }
    socket.shutdown(Shutdown::Both).expect("stream shutdown failed");
}

fn save_reads(
    map: &mut HashMap<u128, (u128, TagData)>,
    control: &Arc<Mutex<control::Control>>,
    sqlite: &Arc<Mutex<sqlite::SQLite>>,
    r_name: &str
) {
    let mut reads: Vec<read::Read> = Vec::new();
    for (_, old_tag) in map.values() {
        let mut chip_type = String::from(defaults::DEFAULT_CHIP_TYPE);
        if let Ok(control) = control.lock() {
            control.chip_type.clone_into(&mut chip_type);
        }
        let chip = if chip_type == types::TYPE_CHIP_DEC {format!("{}", old_tag.tag)} else {format!("{:x}", old_tag.tag)};
        reads.push(read::Read::new(
            0,
            chip,
            (old_tag.portal_time / 1000000) as u64,
            ((old_tag.portal_time / 1000) % 1000) as u32,
            (old_tag.reader_time / 1000000) as u64,
            ((old_tag.reader_time / 1000) % 1000) as u32,
            old_tag.antenna as u32,
            String::from(r_name),
            format!("{}", old_tag.rssi),
            0,
            0
        ));
    }
    if reads.len() > 0 {
        
        match sqlite.lock() {
            Ok(mut db) => {
                match db.save_reads(&reads) {
                    Ok(_num) => {
                        //println!("Saved {_num} reads.")
                    },
                    Err(e) => println!("Error saving reads. {e}"),
                }
            },
            Err(e) => {
                println!("Error saving reads on thread close. {e}");
            }
        }
    }
}

fn send_antennas(
    reader_name: &str,
    antennas: &Arc<Mutex<[u8;MAX_ANTENNAS]>>,
    control_sockets: &Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED+1]>>
) -> Result<(), &'static str> {
    let mut no_error = true;
    if let Ok(sockets) = control_sockets.lock() {
        if let Ok(ant) = antennas.lock() {
            for ix in 0..MAX_CONNECTED {
                match &sockets[ix] {
                    Some(sock) => {
                        no_error = no_error && socket::write_reader_antennas(sock, reader_name.to_string(), &*ant)
                    },
                    None => {}
                }
            }
        } else {
            return Err("error getting antennas mutex")
        }
    } else {
        return Err("error getting sockets mutex")
    }
    if no_error == false {
        return Err("error occurred writing to one or more sockets")
    }
    Ok(())
}

fn send_new(
    reads: Vec<read::Read>,
    control_sockets: &Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED+1]>>,
    read_repeaters: &Arc<Mutex<[bool;MAX_CONNECTED]>>,
) -> Result<(), &'static str> {
    let mut no_error = true;
    if let Ok(sockets) = control_sockets.lock() {
        if let Ok(mut repeaters) = read_repeaters.lock() {
            for ix in 0..MAX_CONNECTED {
                match &sockets[ix] {
                    Some(sock) => {
                        if repeaters[ix] == true {
                            //println!("Sending reads to subscribed socket {ix}.");
                            // If write_reads returned false it wasn't able to write the reads due to connection being broken.
                            let loc_err = socket::write_reads(&sock, &reads);
                            if !loc_err {
                                repeaters[ix] = false;
                                if let Err(e) = sock.shutdown(std::net::Shutdown::Both) {
                                    println!("Error shutting down closed socket. {e}");
                                }
                            }
                            no_error = no_error && loc_err;
                        }
                    },
                    None => {}
                }
            }
        } else {
            return Err("error getting repeaters mutex")
        }
    } else {
        return Err("error getting sockets mutex")
    }
    if no_error == false {
        return Err("error occurred writing to one or more sockets")
    }
    Ok(())
}

fn process_tags(
    map: &mut HashMap<u128, (u128, TagData)>,
    tags: &mut Vec<TagData>,
    control: &Arc<Mutex<control::Control>>,
    read_saver: &Arc<processor::ReadSaver>,
    r_name: &str
) -> Result<Vec<read::Read>, &'static str> {
    let since_epoch = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(v) => v.as_micros() as u64,
        Err(_) => return Err("something went wrong trying to get current time")
    };
    // get the read window from 1/10 of a second to milliseconds
    let mut window = (defaults::DEFAULT_READ_WINDOW as u128) * 100000;
    if let Ok(control) = control.lock() {
        window = (control.read_window as u128) * 100000;
    }
    let one_second = 1000000;
    // sort tags so the earliest seen are first
    tags.sort_by(|a, b| a.portal_time.cmp(&b.portal_time));
    let mut reads: Vec<read::Read> = Vec::new();
    let mut chip_type = String::from(defaults::DEFAULT_CHIP_TYPE);
    if let Ok(control) = control.lock() {
        control.chip_type.clone_into(&mut chip_type);
    }
    for tag in tags {
        // check if the map contains the tag
        if map.contains_key(&tag.tag) {
            let (fs, old_tag) = match map.remove(&tag.tag) {
                Some(v) => v,
                None => return Err("didn't find data we expected")
            };
            // check if we're in the window
            // First Seen + Window is a value greater than when we've seen this tag
            // then we are in the window
            if fs + window > tag.portal_time {
                // if our new tag has a higher rssi we want to record it
                if tag.rssi > old_tag.rssi {
                    map.insert(tag.tag, (fs, TagData{
                        tag: tag.tag,
                        rssi: tag.rssi,
                        antenna: tag.antenna,
                        first_seen: fs,
                        last_seen: tag.last_seen,
                        reader_time: tag.reader_time,
                        portal_time: tag.portal_time,
                    }));
                } else {
                    map.insert(tag.tag, (fs, old_tag));
                }
            // otherwise we can save the old value and start a new one for this tag
            } else {
                let chip = if chip_type == types::TYPE_CHIP_DEC {format!("{}", old_tag.tag)} else {format!("{:x}", old_tag.tag)};
                reads.push(read::Read::new(
                    0,
                    chip,
                    (old_tag.portal_time / 1000000) as u64,
                    ((old_tag.portal_time / 1000) % 1000) as u32,
                    (old_tag.reader_time / 1000000) as u64,
                    ((old_tag.reader_time / 1000) % 1000) as u32,
                    old_tag.antenna as u32,
                    String::from(r_name),
                    format!("{}", old_tag.rssi),
                    read::READ_STATUS_UNUSED,
                    read::READ_UPLOADED_FALSE
                ));
                map.insert(tag.tag, (tag.portal_time, TagData{
                    tag: tag.tag,
                    rssi: tag.rssi,
                    antenna: tag.antenna,
                    first_seen: tag.first_seen,
                    last_seen: tag.last_seen,
                    reader_time: tag.reader_time,
                    portal_time: tag.portal_time,
                }));
            }
        // else add the tag to the map
        } else {
            map.insert(tag.tag, (tag.portal_time, TagData{
                tag: tag.tag,
                rssi: tag.rssi,
                antenna: tag.antenna,
                first_seen: tag.first_seen,
                last_seen: tag.last_seen,
                reader_time: tag.reader_time,
                portal_time: tag.portal_time,
            }));
        }
    }
    let mut removed: Vec<u128> = Vec::new();
    for (fs, old_tag) in map.values() {
        // if we're 1 second past the window
        if fs + window + one_second < since_epoch.into() {
            let chip = if chip_type == types::TYPE_CHIP_DEC {format!("{}", old_tag.tag)} else {format!("{:x}", old_tag.tag)};
            reads.push(read::Read::new(
                0,
                chip,
                (old_tag.portal_time / 1000000) as u64,
                ((old_tag.portal_time / 1000) % 1000) as u32,
                (old_tag.reader_time / 1000000) as u64,
                ((old_tag.reader_time / 1000) % 1000) as u32,
                old_tag.antenna as u32,
                String::from(r_name),
                format!("{}", old_tag.rssi),
                read::READ_STATUS_UNUSED,
                read::READ_UPLOADED_FALSE
            ));
            removed.push(old_tag.tag);
        }
    }
    for to_remove in removed {
        map.remove(&to_remove);
    }
    if reads.len() > 0 {
        // upload reads to database
        if let Err(_) = read_saver.save_reads(&reads) {
            println!("something went wrong saving reads");
        }
    }
    Ok(reads)
}

fn stop_reading(t_stream: &mut TcpStream, msg_id: u32) -> Result<(), &'static str> {
    // disable rospec
    let msg = requests::disable_rospec(&msg_id, &0);
    match t_stream.write_all(&msg) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // delete rospec
    let msg = requests::delete_rospec(&(msg_id + 1), &0);
    match t_stream.write_all(&msg) {
        Ok(_) => Ok(()),
        Err(_) => return Err("unable to write to stream"),
    }
}

fn finalize(
    t_stream: &mut TcpStream,
    msg_id: &Arc<sync::Mutex<u32>>,
    status: &Arc<sync::Mutex<ReaderStatus>>,
    last_ka_received_at: u64
) {
    // finalize what we're doing
    let mut fin_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    if let Ok(r) = status.lock() {
        if ReaderStatus::Disconnected != *r {
            match stop_reading(t_stream, fin_id) {
                Ok(_) => (),
                Err(e) => println!("Error trying to stop reading. {e}"),
            };
            fin_id = fin_id + 2;
        }
    }
    let close = requests::close_connection(&fin_id);
    let buf: &mut [u8; BUFFER_SIZE] = &mut [0;BUFFER_SIZE];
    let leftover_buffer: &mut [u8; BUFFER_SIZE] = &mut [0; BUFFER_SIZE];
    let leftover_num: &mut usize = &mut 0;
    match t_stream.write_all(&close) {
        Ok(_) => {
            match read(t_stream, buf, leftover_buffer, leftover_num, last_ka_received_at) {
                Ok(_) => (),
                Err(e) => {
                    match e.kind() {
                        ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset | ErrorKind::TimedOut | ErrorKind::WouldBlock => (),
                        _ => println!("Error reading from reader. {e}"),
                    }
                }
            }
        },
        Err(e) => println!("Error closing connection. {e}"),
    }
}

fn send_set_keepalive(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // set reader configuration     - set keepalive
    let buf = requests::set_keepalive(&local_id);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}

fn send_purge_tags(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // purge tags
    let buf = requests::purge_tags(&local_id);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}

fn send_set_no_filter(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // purge tags
    let buf = requests::set_no_filter(&local_id);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}

fn send_set_reader_config(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // set reader configuration     - normal config
    let buf = requests::set_reader_config(&local_id);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}

fn send_enable_events_and_reports(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // enable events and reports
    let buf = requests::enable_events_and_reports(&local_id);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}


fn send_get_reader_config(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    let local_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    // get antenna properties (config == 2)
    // this will report back information on the antennas
    // gpi_port and gpo_port values should be ignored in this query
    let buf = requests::get_reader_config(&local_id, &0, &2, &0, &0);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // update message id
    if let Ok(mut id) = msg_id.lock() {
        *id += 1;
    } else {
        return Err("unable to get id lock")
    }
    return Ok(())
}

fn read(
    tcp_stream: &mut TcpStream,
    buf: &mut [u8;BUFFER_SIZE],
    leftover_buffer: &mut [u8;BUFFER_SIZE],
    leftover_num: &mut usize,
    last_ka_received_at: u64
) -> Result<ReadData, std::io::Error> {
    let mut output = ReadData {
        tags: Vec::new(),
        antenna_data: false,
        antennas: [0;MAX_ANTENNAS],
        last_ka_received_at,
        status_messages: Vec::new(),
    };
    let mut file: Option<File> = None;
    if let Ok(file_path) = env::var(WRITEABLE_FILE_PATH) {
        file = Some(OpenOptions::new().append(true).create(true).open(file_path).unwrap());
    }
    let numread = tcp_stream.read(buf);
    match numread {
        Ok(num) => {
            let mut cur_ix = 0;
            // process leftovers
            if *leftover_num > 0 {
                if *leftover_num < 4 {
                    let mut copy_amount = 4;
                    if num < 4 {
                        copy_amount = num
                    }
                    leftover_buffer[*leftover_num..(*leftover_num+copy_amount)].copy_from_slice(&buf[..copy_amount])
                }
                if let Ok(leftover_type) = llrp::bit_masks::get_msg_type(leftover_buffer) {
                    cur_ix = (leftover_type.length as usize) - *leftover_num;
                    // only copy over bytes if they'll fit in the buffer, otherwise ignore the leftover data
                    if (*leftover_num + cur_ix) <= BUFFER_SIZE && cur_ix <= num {
                        leftover_buffer[*leftover_num..(*leftover_num+cur_ix)].copy_from_slice(&buf[..cur_ix]);
                        let max_ix = leftover_type.length as usize;
                        match leftover_type.kind {
                            llrp::message_types::KEEPALIVE => {
                                let local_received_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                                if local_received_at > output.last_ka_received_at {
                                    output.last_ka_received_at = local_received_at
                                }
                                let response = requests::keepalive_ack(&leftover_type.id);
                                match tcp_stream.write_all(&response) {
                                    Ok(_) => (),
                                    Err(e) => eprintln!("Error responding to keepalive. {e}"),
                                }
                            },
                            llrp::message_types::RO_ACCESS_REPORT => {
                                match process_tag_read(leftover_buffer, 10, &max_ix) {
                                    Ok(opt_tag) => match opt_tag {
                                        Some(tag) => {
                                            output.tags.push(tag);
                                        },
                                        None => (),
                                    },
                                    Err(_) => (),
                                };
                            },
                            llrp::message_types::GET_READER_CONFIG_RESPONSE => {
                                match process_reader_config(leftover_buffer, 10, &max_ix) {
                                    Ok(antennas) => {
                                        if let Some(ant) = antennas {
                                            output.antennas = ant;
                                            output.antenna_data = true;
                                        };
                                    },
                                    Err(_) => (),
                                }
                            }
                            llrp::message_types::READER_EVENT_NOTIFICATION => {
                                match process_reader_event_notification(leftover_buffer, 10, &max_ix) {
                                    Ok(antenna) => {
                                        if let Some(ant) = antenna {
                                            output.antennas[ant.0] = ant.1;
                                            output.antenna_data = true;
                                        }
                                    },
                                    Err(_) => (),
                                }
                            }, // Processing of initialization and shutdown commands.
                            llrp::message_types::ADD_ROSPEC_RESPONSE |
                            llrp::message_types::ENABLE_ROSPEC_RESPONSE |
                            llrp::message_types::START_ROSPEC_RESPONSE |
                            llrp::message_types::STOP_ROSPEC_RESPONSE |
                            llrp::message_types::DISABLE_ROSPEC_RESPONSE |
                            llrp::message_types::DELETE_ROSPEC_RESPONSE |
                            llrp::message_types::DELETE_ACCESS_SPEC_RESPONSE |
                            llrp::message_types::SET_READER_CONFIG_RESPONSE => {
                                let (success, response_message) = match process_llrp_status_parameter(leftover_buffer, 10, &max_ix)
                                {
                                    Ok(resp) => match resp {
                                        Some(msg) => (false, msg),
                                        None => (true, "success".to_string()),
                                    },
                                    Err(msg) => (false, msg.to_string()),
                                };
                                output.status_messages.push((leftover_type.kind, success));
                                if let Some(ref mut file) = file {
                                    if let Err(e) = writeln!(file, "{} - {response_message}", message_types::get_message_name(leftover_type.kind).unwrap()) {
                                        eprintln!("Couldn't write to file: {}", e);
                                    }
                                }
                            },
                            llrp::message_types::CUSTOM_MESSAGE => {
                                let (success, message_name, response_message) = match process_custom_message(leftover_buffer, 10, &max_ix) {
                                    Ok(resp) => match resp {
                                        Some(msg_info) => match msg_info {
                                            (parameter_types::MOTOROLA_VENDOR_ID, parameter_types::MOTO_PURGE_TAGS_RESPONSE) |
                                            (parameter_types::MOTOROLA_VENDOR_ID, parameter_types::MOTO_UPDATE_RADIO_FIRMWARE_RESPONSE) |
                                            (parameter_types::MOTOROLA_VENDOR_ID, parameter_types::MOTO_UPDATE_RADIO_CONFIG_RESPONSE) |
                                            (parameter_types::MOTOROLA_VENDOR_ID, parameter_types::MOTO_GET_RADIO_UPDATE_STATUS_RESPONSE) => {
                                                match process_llrp_status_parameter(buf, cur_ix + 15, &max_ix) {
                                                    Ok(sub_resp) => {
                                                        match sub_resp {
                                                            Some(msg) => (false, get_llrp_custom_message_name(msg_info.0, msg_info.1), msg),
                                                            None => (true, get_llrp_custom_message_name(msg_info.0, msg_info.1), "success".to_string()),
                                                        }
                                                    },
                                                    Err(msg) => (false, get_llrp_custom_message_name(msg_info.0, msg_info.1), msg.to_string()),
                                                }
                                            },
                                            _ => (false, "UNKNOWN CUSTOM MESSAGE", "unknown vendor/message type".to_string()),
                                        },
                                        None => (false, "UNKNOWN CUSTOM MESSAGE", "no information returned".to_string()),
                                    },
                                    Err(msg) => (false, "UNKNOWN CUSTOM MESSAGE", msg.to_string()),
                                };
                                output.status_messages.push((leftover_type.kind, success));
                                if let Some(ref mut file) = file {
                                    if let Err(e) = writeln!(file, "{message_name} - {response_message}") {
                                        eprintln!("Couldn't write to file: {}", e);
                                    }
                                }
                            },
                            found_type => {
                                //println!("Message Type Found! V: {} - {:?}", leftover_type.version, get_message_name(found_type));
                                if let Some(ref mut file) = file {
                                    if let Err(e) = writeln!(file, "Message Type Found! V: {} - {:?}", leftover_type.version, get_message_name(found_type)) {
                                        eprintln!("Couldn't write to file: {}", e);
                                    }
                                }
                            },
                        }
                    }
                }
            }
            *leftover_num = 0;
            // message could contain multiple messages, so process them all
            while cur_ix < num {
                let msg_type = llrp::bit_masks::get_msg_type(&buf[cur_ix..(cur_ix + 10)]);
                match msg_type {
                    Ok(info) => {
                        let max_ix = cur_ix + info.length as usize;
                        // check if we don't have a full message
                        if max_ix > num {
                            //println!("overflow error -- max_ix {max_ix} num {num} length {} kind {} version {}", info.length, info.kind, info.version);\
                            // copy what we have to the start of the buffer
                            *leftover_num = num - cur_ix;
                            leftover_buffer[..*leftover_num].copy_from_slice(&buf[cur_ix..num]);
                            break;
                        }
                        match info.kind {
                            llrp::message_types::KEEPALIVE => {
                                let local_received_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                                if local_received_at > output.last_ka_received_at {
                                    output.last_ka_received_at = local_received_at
                                }
                                let response = requests::keepalive_ack(&info.id);
                                match tcp_stream.write_all(&response) {
                                    Ok(_) => (),
                                    Err(e) => println!("Error responding to keepalive. {e}"),
                                }
                            },
                            llrp::message_types::RO_ACCESS_REPORT => {
                                match process_tag_read(&buf, cur_ix + 10, &max_ix) {
                                    Ok(opt_tag) => match opt_tag {
                                        Some(tag) => {
                                            output.tags.push(tag);
                                        },
                                        None => (),
                                    },
                                    Err(_) => (),
                                };
                            },
                            llrp::message_types::GET_READER_CONFIG_RESPONSE => {
                                match process_reader_config(&buf, cur_ix + 10, &max_ix) {
                                    Ok(antennas) => {
                                        if let Some(ant) = antennas {
                                            output.antennas = ant;
                                            output.antenna_data = true;
                                        };
                                    },
                                    Err(_) => (),
                                }
                            },
                            llrp::message_types::READER_EVENT_NOTIFICATION => {
                                match process_reader_event_notification(&buf, cur_ix + 10, &max_ix) {
                                    Ok(antenna) => {
                                        if let Some(ant) = antenna {
                                            output.antennas[ant.0] = ant.1;
                                            output.antenna_data = true;
                                        }
                                    },
                                    Err(_) => (),
                                }
                            }, // Processing of initialization and shutdown commands.
                            llrp::message_types::ADD_ROSPEC_RESPONSE |
                            llrp::message_types::ENABLE_ROSPEC_RESPONSE |
                            llrp::message_types::START_ROSPEC_RESPONSE |
                            llrp::message_types::STOP_ROSPEC_RESPONSE |
                            llrp::message_types::DISABLE_ROSPEC_RESPONSE |
                            llrp::message_types::DELETE_ROSPEC_RESPONSE |
                            llrp::message_types::DELETE_ACCESS_SPEC_RESPONSE |
                            llrp::message_types::SET_READER_CONFIG_RESPONSE => {
                                let (success, response_message) = match process_llrp_status_parameter(&buf, cur_ix + 10, &max_ix) {
                                    Ok(resp) => match resp {
                                        Some(msg) => (false, msg),
                                        None => (true, "success".to_string()),
                                    },
                                    Err(msg) => (false, msg.to_string()),
                                };
                                output.status_messages.push((info.kind, success));
                                if let Some(ref mut file) = file {
                                    if let Err(e) = writeln!(file, "{} - {response_message}", message_types::get_message_name(info.kind).unwrap()) {
                                        eprintln!("Couldn't write to file: {}", e);
                                    }
                                }
                            },
                            llrp::message_types::CUSTOM_MESSAGE => {
                                let (success, message_name, response_message) = match process_custom_message(&buf, cur_ix + 10, &max_ix) {
                                    Ok(resp) => match resp {
                                        Some(msg_info) => match msg_info {
                                            (parameter_types::MOTOROLA_VENDOR_ID, parameter_types::MOTO_PURGE_TAGS_RESPONSE) |
                                            (parameter_types::MOTOROLA_VENDOR_ID, parameter_types::MOTO_UPDATE_RADIO_FIRMWARE_RESPONSE) |
                                            (parameter_types::MOTOROLA_VENDOR_ID, parameter_types::MOTO_UPDATE_RADIO_CONFIG_RESPONSE) |
                                            (parameter_types::MOTOROLA_VENDOR_ID, parameter_types::MOTO_GET_RADIO_UPDATE_STATUS_RESPONSE) => {
                                                match process_llrp_status_parameter(buf, cur_ix + 15, &max_ix) {
                                                    Ok(sub_resp) => {
                                                        match sub_resp {
                                                            Some(msg) => (false, get_llrp_custom_message_name(msg_info.0, msg_info.1), msg),
                                                            None => (true, get_llrp_custom_message_name(msg_info.0, msg_info.1), "success".to_string()),
                                                        }
                                                    },
                                                    Err(msg) => (false, get_llrp_custom_message_name(msg_info.0, msg_info.1), msg.to_string()),
                                                }
                                            },
                                            _ => (false, "UNKNOWN CUSTOM MESSAGE", "unknown vendor/message type".to_string()),
                                        },
                                        None => (false, "UNKNOWN CUSTOM MESSAGE", "no information returned".to_string()),
                                    },
                                    Err(msg) => (false, "UNKNOWN CUSTOM MESSAGE", msg.to_string()),
                                };
                                output.status_messages.push((info.kind, success));
                                if let Some(ref mut file) = file {
                                    if let Err(e) = writeln!(file, "{message_name} - {response_message}") {
                                        eprintln!("Couldn't write to file: {}", e);
                                    }
                                }
                            },
                            found_type => {
                                //println!("Message Type Found! V: {} - {:?}", info.version, get_message_name(found_type));
                                if let Some(ref mut file) = file {
                                    if let Err(e) = writeln!(file, "Unknown message Type Found! V: {} - {:?}", info.version, get_message_name(found_type)) {
                                        eprintln!("Couldn't write to file: {}", e);
                                    }
                                }
                            },
                        }
                        cur_ix = max_ix;
                    },
                    Err(e) => {
                        return Err(std::io::Error::new(ErrorKind::InvalidData, e))
                    },
                }
            }
        }
        Err(e) => {
            return Err(e);
        },
    }
    Ok(output)
}

#[derive(Debug)]
pub struct TagData {
    tag: u128,              // 96 bits possible
    antenna: u16,           // short integer
    rssi: i8,               // possible values -128 to +127
    first_seen: u128,       // time since 00:00:00 UTC Jan 1 1970 in microseconds (1,000,000 per second, 1,000 per millisecond)
    last_seen: u128,        // time since 00:00:00 UTC Jan 1 1970 in microseconds
    reader_time: u128,
    portal_time: u128,      // time since 00:00:00 UTC Jan 1 1970 in microseconds (1,000,000 per second, 1,000 per millisecond)
}

fn process_reader_event_notification(buf: &[u8;BUFFER_SIZE], start_ix: usize, max_ix: &usize) -> Result<Option<(usize, u8)>, &'static str> {
    let mut bits = ((buf[start_ix] as u32) << 24) +
           ((buf[start_ix+1] as u32) << 16) +
           ((buf[start_ix+2] as u32) << 8) +
            (buf[start_ix+3] as u32);
    let mut param_info = match llrp::bit_masks::get_param_type(&bits) {
        Ok(info) => info,
        Err(_) => return Err("unable to get parameter info"),
    };
    if parameter_types::READER_EVENT_NOTIFICATION_DATA != param_info.kind {
        return Err("invalid tlv parameter")
    }
    let mut param_ix = start_ix + 4;
    let mut output: Option<(usize, u8)> = None;
    while param_ix < *max_ix {
        bits = ((buf[param_ix] as u32) << 24) +
               ((buf[param_ix+1] as u32) << 16) +
               ((buf[param_ix+2] as u32) << 8) +
                (buf[param_ix+3] as u32);
        param_info = match llrp::bit_masks::get_param_type(&bits) {
            Ok(info) => info,
            Err(_) => return Err("unable to get parameter info"),
        };
        match param_info.kind {
            parameter_types::UTC_TIMESTAMP => { },
            parameter_types::ANTENNA_EVENT => {
                // bytes 0, 1, 2, 3 are the TLV Parameter information, type and length -- ignore
                // byte 4 is the connected bit, 0x00 if not connected, 0x01 if connected
                // bytes 5 and 6 are the antenna number, 0x00 0x01, 6 should be the only one that matters
                let mut number = ((buf[param_ix+5] as usize) << 8) + (buf[param_ix+6] as usize);
                if number > MAX_ANTENNAS {
                    return Err("antenna number greater than the max number of antennas supported")
                } else if number > 0 {
                    number -= 1;
                }
                output = match buf[param_ix+4] {
                    0x00 => Some((number, ANTENNA_STATUS_DISCONNECTED)),
                    _ => Some((number, ANTENNA_STATUS_CONNECTED)),
                };
            },
            _ => { },
        }
        param_ix += param_info.length as usize;
    }
    Ok(output)
}

fn process_custom_message(buf: &[u8;BUFFER_SIZE], start_ix: usize, max_ix: &usize) -> Result<Option<(u32, u16)>, &'static str> {
    // first 32 bits are the vendor identifier
    // next 8 bits are the message subtype
    // the leftover bits are the vendor specified payload
    if *max_ix < start_ix + 4 {
        return Err("invalid length")
    }
    let vendor_id = ((buf[start_ix] as u32) << 24) +
            ((buf[start_ix+1] as u32) << 16) +
            ((buf[start_ix+2] as u32) << 8) +
            (buf[start_ix+3] as u32);
    let subtype = buf[start_ix+4] as u16;
    return Ok(Some((vendor_id, subtype)));
}

fn process_llrp_status_parameter(buf: &[u8;BUFFER_SIZE], start_ix: usize, max_ix: &usize) -> Result<Option<String>, &'static str> {
    // ---------- LLRPStatus Parameter ----------
    // first 6 bits are reserved
    // next 10 bits are Type (287)
    // next 16 bits are the length of the message
    // next 16 bits are status code
    // next 16 bits are are error description bytecount (BC)
    // what follows is BC bytes length error description as UTF-8 String
    // optionally followed by FieldError Parameter
            // first 6 bits are reserved
            // next 10 bits are type (288)
            // next 16 bits are the length of the parameter (8 bytes)
            // next 16 bits are the FieldNum (field number for which the error applies)
            // followed by a 16 bit integer specifying the error code (found under LLRP Status Codes)
    // optionally followed by ParameterError Parameter
            // first 6 bits are reserved
            // next 10 bits are type (289)
            // next 16 bits specify the parameter type that caused the error
            // next 16 bits are the error code (possible values under LLRP Status Codes)
            // optionally followed by FieldError Parameter
            // optionally followed by ParameterError Parameter
    let bits = ((buf[start_ix] as u32) << 24) +
            ((buf[start_ix+1] as u32) << 16) +
            ((buf[start_ix+2] as u32) << 8) +
            (buf[start_ix+3] as u32);
    let param_info = match llrp::bit_masks::get_param_type(&bits) {
        Ok(info) => info,
        Err(_) => return Err("unable to get parameter info"),
    };
    if parameter_types::LLRP_STATUS != param_info.kind {
        println!("invalid llrp status parameter parsed: {}", param_info.kind);
        return Err("invalid llrp status parameter")
    }
    let mut param_ix = start_ix + 4;
    let mut output: Option<String> = None;
    let code: u16 = ((buf[param_ix] as u16) << 8) +
            (buf[param_ix+1] as u16);
    if parameter_types::M_SUCCESS != code {
        let status_name = match parameter_types::get_llrp_status_name(code) {
            Some(stat) => stat,
            None => "UNKNOWN"
        };
        let error_description_bytecount: usize = ((buf[param_ix+2] as usize) << 8) +
                (buf[param_ix+3] as usize);
        param_ix += 4;
        if param_ix + error_description_bytecount + 1 > *max_ix {
            return Err("error message length longer than parameter reported length")
        }
        let error_description = match str::from_utf8(&buf[param_ix..param_ix+error_description_bytecount+1]) {
            Ok(desc) => desc,
            Err(_) => return Err("unable to convert error description to string")
        };
        output = Some(format!("{status_name}: {error_description}"));
        // potentially process FieldError Parameter and ParameterError Parameter after this
    }
    return Ok(output)
}

fn process_reader_config(buf: &[u8;BUFFER_SIZE], start_ix: usize, max_ix: &usize) -> Result<Option<[u8;MAX_ANTENNAS]>, &'static str> {
    let mut bits: u32;
    let mut param_info: ParamTypeInfo;
    let mut param_ix = start_ix;
    let mut output: [u8;MAX_ANTENNAS] = [0;MAX_ANTENNAS];
    let mut antenna_found = false;
    while param_ix < *max_ix {
        bits = ((buf[param_ix] as u32) << 24) +
               ((buf[param_ix+1] as u32) << 16) +
               ((buf[param_ix+2] as u32) << 8) +
                (buf[param_ix+3] as u32);
        param_info = match llrp::bit_masks::get_param_type(&bits) {
            Ok(info) => info,
            Err(_) => return Err("unable to get parameter info"),
        };
        match param_info.kind {
            parameter_types::ANTENNA_PROPERTIES => {
                // bytes 0, 1, 2, 3 are the TLV Parameter information, type and length -- ignore
                // byte 4 is the connected bit, 0x00 if not connected, 0x80 if connected
                // bytes 5 and 6 are the antenna number, 0x00 0x01, 6 should be the only one that matters
                // bytes 7 and 8 are the antenna gain -- ignore
                let mut number = ((buf[param_ix+5] as usize) << 8) + (buf[param_ix+6] as usize);
                if number > MAX_ANTENNAS {
                    return Err("antenna number greater than the max number of antennas supported")
                } else if number > 0 {
                    number -= 1;
                }
                output[number] = match buf[param_ix+4] {
                    0x00 => ANTENNA_STATUS_DISCONNECTED,
                    _ => ANTENNA_STATUS_CONNECTED,
                };
                antenna_found = true;
            },
            parameter_types::ANTENNA_CONFIGURATION => { },
            parameter_types::READER_EVENT_NOTIFICATION_SPEC => { },
            parameter_types::RO_REPORT_SPEC => { },
            parameter_types::ACCESS_REPORT_SPEC => { },
            parameter_types::LLRP_CONFIGURATION_STATE_VALUE => { },
            parameter_types::KEEPALIVE_SPEC => { },
            parameter_types::GPI_PORT_CURRENT_STATE => { },
            parameter_types::GPO_WRITE_DATA => { },
            parameter_types::CUSTOM_PARAMETER => { },
            parameter_types::LLRP_STATUS => { },
            parameter_types::IDENTIFICATION => { },
            other => {
                println!("unknown parameter type found: {:?}", other);
            }
        }
        param_ix += param_info.length as usize;
    }
    if !antenna_found {
        return Ok(None)
    }
    Ok(Some(output))
}

fn process_tag_read(buf: &[u8;BUFFER_SIZE], start_ix: usize, max_ix: &usize) -> Result<Option<TagData>, &'static str> {
    let mut bits: u32 = ((buf[start_ix] as u32) << 24) +
                    ((buf[start_ix+1] as u32) << 16) +
                    ((buf[start_ix+2] as u32) << 8) +
                    (buf[start_ix+3] as u32);
    let mut param_info = match llrp::bit_masks::get_param_type(&bits) {
        Ok(info) => info,
        Err(_) => return Err("unable to get parameter info"),
    };
    // verify we actually got tag data
    if param_info.kind != parameter_types::TAG_REPORT_DATA {
        return Ok(None)
    }
    if param_info.length < 5 {
        return Ok(None)
    }
    // gather 
    let mut data: TagData = TagData {
        tag: 0,
        antenna: 0,
        rssi: 0,
        first_seen: 0,
        last_seen: 0,
        reader_time: 0,
        portal_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros(),
    };
    let mut param_ix = start_ix + 4;
    while param_ix < *max_ix {
        bits = u32::from_be_bytes([buf[param_ix], buf[param_ix+1], buf[param_ix+2], buf[param_ix+3]]);
        param_info = match llrp::bit_masks::get_param_type(&bits) {
            Ok(info) => info,
            Err(_) => return Err("unable to get parameter info"),
        };
        match param_info.kind {
            // don't need these next three
            parameter_types::RO_SPEC_ID => { },
            parameter_types::C1G2_PC => { },
            parameter_types::C1G2_CRC => { },
            // need these
            parameter_types::EPC_96 => {
                data.tag = ((buf[param_ix+1] as u128) << 88) +
                        ((buf[param_ix+2] as u128) << 80) +
                        ((buf[param_ix+3] as u128) << 72) +
                        ((buf[param_ix+4] as u128) << 64) +
                        ((buf[param_ix+5] as u128) << 56) +
                        ((buf[param_ix+6] as u128) << 48) +
                        ((buf[param_ix+7] as u128) << 40) +
                        ((buf[param_ix+8] as u128) << 32) +
                        ((buf[param_ix+9] as u128) << 24) +
                        ((buf[param_ix+10] as u128) << 16) +
                        ((buf[param_ix+11] as u128) << 8) +
                        (buf[param_ix+12] as u128);
            },
            parameter_types::ANTENNA_ID => {
                data.antenna = ((buf[param_ix+1] as u16) << 8) +
                            (buf[param_ix+2] as u16);
            },
            parameter_types:: PEAK_RSSI => {
                data.rssi = buf[param_ix+1] as i8;
            },
            parameter_types::FIRST_SEEN_TIMESTAMP_UTC => {
                data.reader_time = ((buf[param_ix+1] as u128) << 56) +
                                ((buf[param_ix+2] as u128) << 48) +
                                ((buf[param_ix+3] as u128) << 40) +
                                ((buf[param_ix+4] as u128) << 32) +
                                ((buf[param_ix+5] as u128) << 24) +
                                ((buf[param_ix+6] as u128) << 16) +
                                ((buf[param_ix+7] as u128) << 8) +
                                (buf[param_ix+8] as u128);
            },
            parameter_types::LAST_SEEN_TIMESTAMP_UTC => {
                data.last_seen = ((buf[param_ix+1] as u128) << 56) +
                                ((buf[param_ix+2] as u128) << 48) +
                                ((buf[param_ix+3] as u128) << 40) +
                                ((buf[param_ix+4] as u128) << 32) +
                                ((buf[param_ix+5] as u128) << 24) +
                                ((buf[param_ix+6] as u128) << 16) +
                                ((buf[param_ix+7] as u128) << 8) +
                                (buf[param_ix+8] as u128);
            },
            _ => {
                //println!("Unknown value found.")
            }
        }
        param_ix += param_info.length as usize;
    }
    Ok(Some(data))
}

fn _process_parameters(buf: &[u8;BUFFER_SIZE], start_ix: usize, num: &usize) {
    let mut start: usize = start_ix;
    while start < *num {
        let bits: u32 = ((buf[start] as u32) << 24) +
                        ((buf[start+1] as u32) << 16) +
                        ((buf[start+2] as u32) << 8) +
                        (buf[start+3] as u32);
        let param_info = match llrp::bit_masks::get_param_type(&bits) {
            Ok(info) => info,
            Err(e) => {
                println!("Unable to process parameters. {e}");
                return
            }
        };
        if param_info.length < 1 {
            return
        }
        match param_info.kind {
            parameter_types::RO_SPEC => {
                if start + 10 > *num {
                    println!("Out of bounds.");
                    return
                }
                // ID is an unsigned integer. 0 is invalid
                let rospec_id: u32 = ((buf[start+4] as u32) << 24) +
                                    ((buf[start+5] as u32) << 16) +
                                    ((buf[start+6] as u32) << 8) +
                                    (buf[start+7] as u32);
                // Valid priorities are 0-7, lower are given higher priority
                let priority: u8 = buf[start+8];
                // 0 = disabled, 1 = inactive, 2 = active
                let current_state: u8 = buf[start+9];
                // 10 is a ROBoundarySpec parameter followed by 1-n SpecParameters followed by 0-1 ROReportSpec parameters
                println!("RO_SPEC Parameter -- id {} - priority {} - current state {}", rospec_id, priority, current_state);
            },
            parameter_types::LLRP_STATUS => {
                if start + 8 > *num {
                    println!("Out of bounds.");
                    return
                }
                // Status code          - integer
                let status_code: u16 = ((buf[start+4] as u16) << 8) + (buf[start+5] as u16);
                // byte count for error description
                let err_des_byte_count: u16 = ((buf[start+5] as u16) << 8) + (buf[start+7] as u16);
                // Error Description    - UTF8 string
                let param_ix = start + 8 + err_des_byte_count as usize;
                let err_des: &str = match str::from_utf8(&buf[start+8..param_ix]) {
                    Ok(v) => v,
                    Err(e) => {
                        println!("Error converting error description. {e}");
                        return
                    }
                };
                println!("LLRP_STATUS parameter - Code {} - Descr {}", status_code, err_des);
                // check if more available to read
                let end: usize = param_info.length as usize + start;
                if end < *num {
                    _process_parameters(buf, start+24, &end)
                }
            },
            parameter_types::ACCESS_SPEC => {
                if start + 24 > *num {
                    println!("Out of bounds.");
                    return
                }
                let spec_id: u32 = ((buf[start+4] as u32) << 24) +
                                    ((buf[start+5] as u32) << 16) +
                                    ((buf[start+6] as u32) << 8) +
                                    (buf[start+7] as u32);
                let antenna_id: u16 = ((buf[start+8] as u16) << 8) +
                                    (buf[start+9] as u16);
                let protocol_id: u8 = buf[start+10];
                let active: bool = (buf[start+11] & 0x80) != 0;
                let rospec_id: u32 = ((buf[start+12] as u32) << 24) +
                                    ((buf[start+13] as u32) << 16) +
                                    ((buf[start+14] as u32) << 8) +
                                    (buf[start+15] as u32);
                let ass_trigger: u32 = ((buf[start+16] as u32) << 24) +
                                    ((buf[start+17] as u32) << 16) +
                                    ((buf[start+18] as u32) << 8) +
                                    (buf[start+19] as u32);
                let access_command: u32 = ((buf[start+20] as u32) << 24) +
                                        ((buf[start+21] as u32) << 16) +
                                        ((buf[start+22] as u32) << 8) +
                                        (buf[start+23] as u32);
                println!("ACCESS_SPEC parameter. Spec {}, Ant {}, Prot {}, Act {}, ROSpec {}, ASSTrigger {}, AccessCommand {}",
                        spec_id,
                        antenna_id,
                        protocol_id,
                        active,
                        rospec_id,
                        ass_trigger,
                        access_command
                    );
                // check if more available to read
                let end: usize = param_info.length as usize + start;
                if end < *num {
                    _process_parameters(buf, start+24, &end)
                }
            },
            parameter_types::READER_EVENT_NOTIFICATION_DATA => {
                // Timestamp Parameter
                // Hopping Event Parameter ?
                // GPIEvent Parameter ?
                // ROSpecEvent Parameter ?
                // ReportBufferLevelWarningEvent Parameter ?
            }
            _ => {
                println!("Parameter found -- {:?} -- TV? {}", parameter_types::get_parameter_name(param_info.kind), param_info.tv);
            }
        }
        start = start + param_info.length as usize;
    }
}
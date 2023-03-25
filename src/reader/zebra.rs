use std::{str::{self, FromStr}, net::{TcpStream, SocketAddr, IpAddr}, thread::{self, JoinHandle}, sync::{self, Arc, Mutex}, io::Read, io::{Write, ErrorKind}, collections::HashMap, time::{SystemTime, UNIX_EPOCH}};
use std::time::Duration;

use crate::{llrp::{self, parameter_types}, database::{sqlite, Database}, objects::read, types, control::{self, socket::{MAX_CONNECTED, self}}};

pub mod requests;

pub const DEFAULT_ZEBRA_PORT: u16 = 5084;

pub fn connect(reader: &mut super::Reader, sqlite: &Arc<Mutex<sqlite::SQLite>>, controls: &control::Control) -> Result<JoinHandle<()>, &'static str> {
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
            // try to send connection messages
            match send_connect_messages(&mut tcp_stream, &reader.msg_id) {
                Ok(_) => println!("Successfully connected to reader {}.", reader.nickname()),
                Err(e) => return Err(e),
            };
            // copy tcp stream into the mutex
            reader.socket = match tcp_stream.try_clone() {
                Ok(stream) => sync::Mutex::new(Some(stream)),
                Err(_) => {
                    return Err("error copying stream to thread")
                }
            };
            if let Ok(mut con) = reader.connected.lock() {
                *con = true;
            }
            // copy values for out thread
            let mut t_stream = tcp_stream;
            let t_mutex = reader.keepalive.clone();
            let msg_id = reader.msg_id.clone();
            let reading = reader.reading.clone();
            let t_reader_name = reader.nickname.clone();
            let t_sqlite = sqlite.clone();
            let t_window = controls.read_window.clone();
            let t_chip_type = controls.chip_type.clone();
            let t_connected = reader.connected.clone();
            let t_control_sockets = reader.control_sockets.clone();
            let t_read_repeaters = reader.read_repeaters.clone();
            let mut t_sight_processor = reader.sight_processor.clone();

            let output = thread::spawn(move|| {
                let buf: &mut [u8; 51200] = &mut [0;51200];
                match t_stream.set_read_timeout(Some(Duration::from_secs(1))) {
                    Ok(_) => (),
                    Err(e) => {
                        println!("Error setting read timeout. {e}")
                    }
                }
                let mut read_map: HashMap<u128, (u64, TagData)> = HashMap::new();
                loop {
                    if let Ok(keepalive) = t_mutex.lock() {
                        // check if we've been told to quit
                        if *keepalive == false {
                            break;
                        };
                    } else {
                        // unable to grab mutex...
                        break;
                    }
                    match read(&mut t_stream, buf) {
                        Ok(mut tags) => {
                            match process_tags(&mut read_map, &mut tags, t_window, &t_chip_type, &t_sqlite, t_reader_name.as_str()) {
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
                        },
                        Err(e) => {
                            match e.kind() {
                                ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset => {
                                    break;
                                }
                                // TimedOut == Windows, WouldBlock == Linux
                                ErrorKind::TimedOut | ErrorKind::WouldBlock => {
                                    match process_tags(&mut read_map, &mut Vec::new(), t_window, &t_chip_type, &t_sqlite, t_reader_name.as_str()) {
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
                                },
                                _ => println!("Error reading from reader. {e}"),
                            }
                        }
                    }
                }
                stop(&mut t_stream, &reading, &t_reader_name, &msg_id);
                finalize(&mut t_stream, &msg_id, &reading);
                save_reads(&mut read_map, &t_chip_type, &t_sqlite, t_reader_name.as_str());
                if let Ok(mut con) = t_connected.lock() {
                    *con = false;
                }
                println!("Thread reading from this reader has now closed.")
            });
            Ok(output)
        },
    }
}

pub fn initialize(reader: &mut super::Reader) -> Result<(), &'static str> {
    if let Ok(mut r) = reader.reading.lock() {
        if *r {
            return Err("already reading")
        }
        *r = true;
    } else {
        return Err("unable to check if we're actually reading")
    }
    let del_acs_id = reader.get_next_id();
    let del_ros_id = reader.get_next_id();
    let add_ros_id = reader.get_next_id();
    let ena_ros_id = reader.get_next_id();
    let sta_ros_id = reader.get_next_id();
    if let Ok(stream) = reader.socket.lock() {
        match &*stream {
            Some(s) => {
                let mut w_stream = match s.try_clone() {
                    Ok(v) => v,
                    Err(_) => return Err("unable to copy stream"),
                };
                // delete all access spec
                let msg = requests::delete_access_spec(&del_acs_id, &0);
                match w_stream.write_all(&msg) {
                    Ok(_) => (),
                    Err(_) => return Err("error writing data")
                }
                // delete all rospec
                let msg = requests::delete_rospec(&del_ros_id, &0);
                match w_stream.write_all(&msg) {
                    Ok(_) => (),
                    Err(_) => return Err("error writing data")
                }
                // add rospec
                let msg = requests::add_rospec(&add_ros_id, &100);
                match w_stream.write_all(&msg) {
                    Ok(_) => (),
                    Err(_) => return Err("error writing data")
                }
                // enable rospec
                let msg = requests::enable_rospec(&ena_ros_id, &100);
                match w_stream.write_all(&msg) {
                    Ok(_) => (),
                    Err(_) => return Err("error writing data")
                }
                // start rospec
                let msg = requests::start_rospec(&sta_ros_id, &100);
                match w_stream.write_all(&msg) {
                    Ok(_) => (),
                    Err(_) => return Err("error writing data")
                }
            },
            None => {
                return Err("not connected")
            }
        }
        Ok(())
    } else {
        return Err("unable to get stream mutex")
    }
}


pub fn stop_reader(reader: &mut super::Reader) -> Result<(), &'static str> {
    if let Ok(r) = reader.reading.lock() {
        if !*r {
            return Err("not reading")
        }
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
                    Ok(_) => println!("No longer reading from reader {}", reader.nickname()),
                    Err(e) => return Err(e),
                }
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
    reading: &Arc<Mutex<bool>>,
    nickname: &String,
    msg_mtx: &Arc<sync::Mutex<u32>>
) {
    if let Ok(r) = reading.lock() {
        if !*r {
            return
        }
    }
    let mut msg_id = 0;
    if let Ok(id) = msg_mtx.lock() {
        msg_id = *id+1;
    }
    match stop_reading(socket, msg_id) {
        Ok(_) => println!("No longer reading from reader {}", nickname),
        Err(_) => (),
    }
}

fn save_reads(
    map: &mut HashMap<u128, (u64, TagData)>,
    chip_type: &str,
    sqlite: &Arc<Mutex<sqlite::SQLite>>,
    r_name: &str
) {
    let mut reads: Vec<read::Read> = Vec::new();
    for (_, old_tag) in map.values() {
        let chip = if chip_type == types::TYPE_CHIP_DEC {format!("{}", old_tag.tag)} else {format!("{:x}", old_tag.tag)};
        reads.push(read::Read::new(
            0,
            chip,
            old_tag.first_seen / 1000000,
            ((old_tag.first_seen / 1000) % 1000) as u32,
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
                    Ok(num) => println!("Saved {num} reads."),
                    Err(e) => println!("Error saving reads. {e}"),
                }
            },
            Err(e) => {
                println!("Error saving reads on thread close. {e}");
            }
        }
    }
}

fn send_new(
    reads: Vec<read::Read>,
    control_sockets: &Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED+1]>>,
    read_repeaters: &Arc<Mutex<[bool;MAX_CONNECTED]>>,
) -> Result<(), &'static str> {
    let mut no_error = true;
    if let Ok(sockets) = control_sockets.lock() {
        if let Ok(repeaters) = read_repeaters.lock() {
            for ix in 0..MAX_CONNECTED {
                match &sockets[ix] {
                    Some(sock) => {
                        if repeaters[ix] == true {
                            println!("Sending reads to subscribed socket {ix}.");
                            no_error = no_error && socket::write_reads(&sock, &reads);
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
    map: &mut HashMap<u128, (u64, TagData)>,
    tags: &mut Vec<TagData>,
    read_window: u8,
    chip_type: &str,
    sqlite: &Arc<Mutex<sqlite::SQLite>>,
    r_name: &str
) -> Result<Vec<read::Read>, &'static str> {
    let since_epoch = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(v) => v.as_micros() as u64,
        Err(_) => return Err("something went wrong trying to get current time")
    };
    // get the read window from 1/10 of a second to milliseconds
    let window = (read_window as u64) * 100000;
    let one_second = 1000000;
    // sort tags so the earliest seen are first
    tags.sort_by(|a, b| a.first_seen.cmp(&b.first_seen));
    let mut reads: Vec<read::Read> = Vec::new();
    for tag in tags {
        // check if the map contains the tag
        if map.contains_key(&tag.tag) {
            let (fs, old_data) = match map.remove(&tag.tag) {
                Some(v) => v,
                None => return Err("didn't find data we expected")
            };
            // check if we're in the window
            // First Seen + Window is a value greater than when we've seen this tag
            // then we are in the window
            if fs + window > tag.first_seen {
                // if our new tag has a higher rssi we want to record it
                if tag.rssi > old_data.rssi {
                    map.insert(tag.tag, (fs, TagData{
                        tag: tag.tag,
                        rssi: tag.rssi,
                        antenna: tag.antenna,
                        first_seen: fs,
                        last_seen: tag.last_seen,
                    }));
                } else {
                    map.insert(tag.tag, (fs, old_data));
                }
            // otherwise we can save the old value and start a new one for this tag
            } else {
                let chip = if chip_type == types::TYPE_CHIP_DEC {format!("{}", old_data.tag)} else {format!("{:x}", old_data.tag)};
                reads.push(read::Read::new(
                    0,
                    chip,
                    old_data.first_seen / 1000000,
                    ((old_data.first_seen / 1000) % 1000) as u32,
                    old_data.antenna as u32,
                    String::from(r_name),
                    format!("{}", old_data.rssi),
                    read::READ_STATUS_UNUSED,
                    read::READ_UPLOADED_FALSE
                ));
                map.insert(tag.tag, (tag.first_seen, TagData{
                    tag: tag.tag,
                    rssi: tag.rssi,
                    antenna: tag.antenna,
                    first_seen: tag.first_seen,
                    last_seen: tag.last_seen,
                }));
            }
        // else add the tag to the map
        } else {
            map.insert(tag.tag, (tag.first_seen, TagData{
                tag: tag.tag,
                rssi: tag.rssi,
                antenna: tag.antenna,
                first_seen: tag.first_seen,
                last_seen: tag.last_seen,
            }));
        }
    }
    let mut removed: Vec<u128> = Vec::new();
    for (fs, old_tag) in map.values() {
        // if we're 1 second past the window
        if fs + window + one_second < since_epoch {
            let chip = if chip_type == types::TYPE_CHIP_DEC {format!("{}", old_tag.tag)} else {format!("{:x}", old_tag.tag)};
            reads.push(read::Read::new(
                0,
                chip,
                old_tag.first_seen / 1000000,
                ((old_tag.first_seen / 1000) % 1000) as u32,
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
        if let Ok(mut db) = sqlite.lock() {
            match db.save_reads(&reads) {
                Ok(n) => println!("Successfully saved {n} reads."),
                Err(_) => return Err("something went wrong saving reads"),
            }
        } else {
            return Err("unable to get database lock")
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

fn finalize(t_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>, reading: &Arc<sync::Mutex<bool>>) {
    // finalize what we're doing
    let mut fin_id = match msg_id.lock() {
        Ok(id) => *id,
        Err(_) => 0,
    };
    if let Ok(r) = reading.lock() {
        if *r {
            match stop_reading(t_stream, fin_id) {
                Ok(_) => (),
                Err(e) => println!("Error trying to stop reading. {e}"),
            };
            fin_id = fin_id + 2;
        }
    }
    let close = requests::close_connection(&fin_id);
    let buf: &mut [u8; 51200] = &mut [0;51200];
    match t_stream.write_all(&close) {
        Ok(_) => {
            match read(t_stream, buf) {
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

fn send_connect_messages(tcp_stream: &mut TcpStream, msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), &'static str> {
    // delete access spec           - 0
    let buf = requests::delete_access_spec(&1, &0);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // delete rospec                - 0
    let buf = requests::delete_rospec(&2, &0);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // set reader configuration     - set keepalive
    let buf = requests::set_keepalive(&3);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // purge tags
    let buf = requests::purge_tags(&4);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // set reader configuration     - set no filter
    let buf = requests::set_no_filter(&5);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // set reader configuration     - normal config
    let buf = requests::set_reader_config(&6);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    // enable events and reports
    let buf = requests::enable_events_and_reports(&7);
    match tcp_stream.write_all(&buf) {
        Ok(_) => (),
        Err(_) => return Err("unable to write to stream"),
    }
    if let Ok(mut id) = msg_id.lock() {
        *id = 7;
    } else {
        return Err("unable to get id lock")
    }
    Ok(())
}

fn read(tcp_stream: &mut TcpStream, buf: &mut [u8;51200]) -> Result<Vec<TagData>, std::io::Error> {
    let mut output: Vec<TagData> = Vec::new();
    let numread = tcp_stream.read(buf);
    match numread {
        Ok(num) => {
            let mut cur_ix = 0;
            // message could contain multiple messages, so process them all
            while cur_ix < num {
                let msg_type = llrp::bit_masks::get_msg_type(&buf[cur_ix..(cur_ix + 10)]);
                match msg_type {
                    Ok(info) => {
                        let max_ix = cur_ix + info.length as usize;
                        // error if we're going to go over max buffer length
                        if max_ix > num {
                            return Err(std::io::Error::new(ErrorKind::InvalidData, "overflow error"))
                        }
                        /*let found_type = match llrp::message_types::get_message_name(info.kind) {
                            Some(found) => found,
                            _ => "UNKNOWN",
                        }; // */
                        match info.kind {
                            llrp::message_types::KEEPALIVE => {
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
                                            output.push(tag);
                                        },
                                        None => (),
                                    },
                                    Err(_) => (),
                                };
                            },
                            _ => {
                                //println!("Message Type Found! V: {} - {}", info.version, found_type);
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
    tag: u128,       // 96 bits possible
    antenna: u16,    // short integer
    rssi: i8,        // possible values -128 to +127
    first_seen: u64, // time since 00:00::00 UTC Jan 1 1970 in microseconds (1,000,000 per second, 1,000 per millisecond)
    last_seen: u64,  // time since 00:00::00 UTC Jan 1 1970 in microseconds
}

fn process_tag_read(buf: &[u8;51200], start_ix: usize, max_ix: &usize) -> Result<Option<TagData>, &'static str> {
    let bits: u32 = ((buf[start_ix] as u32) << 24) +
                    ((buf[start_ix+1] as u32) << 16) +
                    ((buf[start_ix+2] as u32) << 8) +
                    (buf[start_ix+3] as u32);
    let param_info = match llrp::bit_masks::get_param_type(&bits) {
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
        last_seen: 0
    };
    let mut param_ix = start_ix + 4;
    while param_ix < *max_ix {
        let tv_type = (buf[param_ix] & 0x7F) as u16;
        match tv_type {
            // don't need these next three
            parameter_types::RO_SPEC_ID => {
                param_ix = param_ix + 5;
            },
            parameter_types::C1G2_PC => {
                param_ix = param_ix + 3;
            },
            parameter_types::C1G2_CRC => {
                param_ix = param_ix + 3;
            },
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
                param_ix = param_ix + 13;
            },
            parameter_types::ANTENNA_ID => {
                data.antenna = ((buf[param_ix+1] as u16) << 8) +
                            (buf[param_ix+2] as u16);
                param_ix = param_ix + 3;
            },
            parameter_types:: PEAK_RSSI => {
                data.rssi = buf[param_ix+1] as i8;
                param_ix = param_ix + 2;
            },
            parameter_types::FIRST_SEEN_TIMESTAMP_UTC => {
                data.first_seen = ((buf[param_ix+1] as u64) << 56) +
                                ((buf[param_ix+2] as u64) << 48) +
                                ((buf[param_ix+3] as u64) << 40) +
                                ((buf[param_ix+4] as u64) << 32) +
                                ((buf[param_ix+5] as u64) << 24) +
                                ((buf[param_ix+6] as u64) << 16) +
                                ((buf[param_ix+7] as u64) << 8) +
                                (buf[param_ix+8] as u64);
                param_ix = param_ix + 9;
            },
            parameter_types::LAST_SEEN_TIMESTAMP_UTC => {
                data.last_seen = ((buf[param_ix+1] as u64) << 56) +
                                ((buf[param_ix+2] as u64) << 48) +
                                ((buf[param_ix+3] as u64) << 40) +
                                ((buf[param_ix+4] as u64) << 32) +
                                ((buf[param_ix+5] as u64) << 24) +
                                ((buf[param_ix+6] as u64) << 16) +
                                ((buf[param_ix+7] as u64) << 8) +
                                (buf[param_ix+8] as u64);
                param_ix = param_ix + 9;
            },
            _ => {
                println!("Unknown value found.")
            }
        }
    }
    Ok(Some(data))
}

fn _process_parameters(buf: &[u8;51200], start_ix: usize, num: &usize) {
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
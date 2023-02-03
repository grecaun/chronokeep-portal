use std::{str, net::TcpStream, thread::{self, JoinHandle}, sync::{self, Arc, Mutex}, io::Read, io::{Write, ErrorKind}, collections::HashMap, time::{SystemTime, UNIX_EPOCH}};
use std::time::Duration;

use crate::{llrp::{self, parameter_types}, database::{sqlite, Database}, objects::read, types, control};

pub mod requests;

pub const DEFAULT_ZEBRA_PORT: u16 = 5084;

pub struct Zebra {
    id: i64,
    nickname: String,
    kind: String,
    ip_address: String,
    port: u16,

    pub socket: sync::Mutex<Option<TcpStream>>,
    pub keepalive: Arc<sync::Mutex<bool>>,
    pub msg_id: Arc<sync::Mutex<u32>>,

    pub reading: Arc<sync::Mutex<bool>>,
    pub connected: Arc<sync::Mutex<bool>>,
}

impl Zebra {
    pub fn new(
        id: i64,
        nickname: String,
        ip_address: String,
        port: u16,
    ) -> Zebra {
        Zebra {
            id,
            kind: String::from(super::READER_KIND_ZEBRA),
            nickname,
            ip_address,
            port,
            socket: sync::Mutex::new(None),
            keepalive: Arc::new(sync::Mutex::new(true)),
            msg_id: Arc::new(sync::Mutex::new(0)),
            reading: Arc::new(sync::Mutex::new(false)),
            connected: Arc::new(sync::Mutex::new(false)),
        }
    }
}

impl super::Reader for Zebra {
    fn set_id(&mut self, id: i64) {
        self.id = id;
    }

    fn id(&self) -> i64 {
        self.id
    }

    fn nickname(&self) -> &str {
        self.nickname.as_str()
    }

    fn kind(&self) -> &str {
        self.kind.as_str()
    }

    fn ip_address(&self) -> &str {
        self.ip_address.as_str()
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn equal(&self, other: &dyn super::Reader) -> bool {
        self.nickname == other.nickname() &&
            self.kind == other.kind() &&
            self.ip_address == other.ip_address() &&
            self.port == other.port()
    }

    fn is_connected(&self) -> Option<bool> {
        let mut output: Option<bool> = None;
        if let Ok(con) = self.connected.lock() {
            output = Some(*con);
        }
        output
    }

    fn is_reading(&self) -> Option<bool> {
        let mut output: Option<bool> = None;
        if let Ok(con) = self.reading.lock() {
            output = Some(*con);
        }
        output
    }

    fn connect(&mut self, sqlite: &Arc<Mutex<sqlite::SQLite>>, controls: &control::Control) -> Result<JoinHandle<()>, &'static str> {
        let res = TcpStream::connect(format!("{}:{}", self.ip_address, self.port));
        match res {
            Err(_) => return Err("unable to connect"),
            Ok(mut tcp_stream) => {
                // try to send connection messages
                match send_connect_messages(&mut tcp_stream, &self.msg_id) {
                    Ok(_) => println!("Successfully connected to reader {}.", self.nickname()),
                    Err(e) => return Err(e),
                };
                // copy tcp stream into the mutex
                self.socket = match tcp_stream.try_clone() {
                    Ok(stream) => sync::Mutex::new(Some(stream)),
                    Err(_) => {
                        return Err("error copying stream to thread")
                    }
                };
                if let Ok(mut con) = self.connected.lock() {
                    *con = true;
                }
                // copy values for out thread
                let mut t_stream = tcp_stream;
                let t_mutex = self.keepalive.clone();
                let msg_id = self.msg_id.clone();
                let reading = self.reading.clone();
                let t_reader_name = self.nickname.clone();
                let t_sqlite = sqlite.clone();
                let t_window = controls.read_window.clone();
                let t_chip_type = controls.chip_type.clone();
                let t_connected = self.connected.clone();

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
                                    Ok(_) => {
                                        //println!("Tags processed.");
                                    },
                                    Err(e) => println!("Error processing tags. {e}"),
                                };
                            },
                            Err(e) => {
                                match e.kind() {
                                    ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset => {
                                        break;
                                    }
                                    ErrorKind::TimedOut => {
                                        match process_tags(&mut read_map, &mut Vec::new(), t_window, &t_chip_type, &t_sqlite, t_reader_name.as_str()) {
                                            Ok(_) => {
                                                //println!("Timeout tags processed.");
                                            },
                                            Err(e) => println!("Error processing tags. {e}"),
                                        }
                                    },
                                    _ => println!("Error reading from reader. {e}"),
                                }
                            }
                        }
                    }
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

    fn disconnect(&mut self) -> Result<(), &'static str> {
        if let Ok(mut keepalive) = self.keepalive.lock() {
            *keepalive = false;
        };
        Ok(())
    }

    fn initialize(&mut self) -> Result<(), &'static str> {
        if let Ok(mut r) = self.reading.lock() {
            if *r {
                return Err("already reading")
            }
            *r = true;
        } else {
            return Err("unable to check if we're actually reading")
        }
        let del_acs_id = self.get_next_id();
        let del_ros_id = self.get_next_id();
        let add_ros_id = self.get_next_id();
        let ena_ros_id = self.get_next_id();
        let sta_ros_id = self.get_next_id();
        if let Ok(stream) = self.socket.lock() {
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

    fn stop(&mut self) -> Result<(), &'static str> {
        if let Ok(r) = self.reading.lock() {
            if !*r {
                return Err("not reading")
            }
        } else {
            return Err("unable to check if we're actually reading")
        }
        let msg_id = self.get_next_id();
        if let Ok(stream) = self.socket.lock() {
            match &*stream {
                Some(s) => {
                    let mut w_stream = match s.try_clone() {
                        Ok(v) => v,
                        Err(_) => return Err("unable to copy stream"),
                    };
                    match stop_reading(&mut w_stream, msg_id) {
                        Ok(_) => println!("No longer reading from reader {}", self.nickname()),
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

    fn send(&mut self, buf: &[u8]) -> Result<(), &'static str> {
        if let Ok(stream) = self.socket.lock() {
            match &*stream {
                Some(s) => {
                    let mut w_stream = match s.try_clone() {
                        Ok(v) => v,
                        Err(_) => return Err("unable to copy stream")
                    };
                    match w_stream.write_all(buf) {
                        Ok(_) => (),
                        Err(_) => return Err("error writing data")
                    }
                    Ok(())
                },
                None => {
                    Err("not connected")
                },
            }
        } else {
            Err("unable to get mutex")
        }
    }

    fn get_next_id(&mut self) -> u32 {
        let mut output: u32 = 0;
        if let Ok(mut v) = self.msg_id.lock() {
            output = *v + 1;
            *v = output;
        }
        output
    }

    fn set_nickname(&mut self, name: String) {
        self.nickname = name;
    }

    fn set_kind(&mut self, kind: String) {
        self.kind = kind;
    }

    fn set_ip_address(&mut self, ip_address: String) {
        self.ip_address = ip_address;
    }

    fn set_port(&mut self, port: u16) {
        self.port = port;
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

fn process_tags(
    map: &mut HashMap<u128, (u64, TagData)>,
    tags: &mut Vec<TagData>,
    read_window: u8,
    chip_type: &str,
    sqlite: &Arc<Mutex<sqlite::SQLite>>,
    r_name: &str
) -> Result<(), &'static str> {
    let since_epoch = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(v) => v.as_micros() as u64,
        Err(_) => return Err("something went wrong trying to get current time")
    };
    // get the read window from 1/10 of a second to milliseconds
    let window = (read_window as u64) * 100000;
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
                        first_seen: tag.first_seen,
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
                    0,
                    0
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
        // if we're double the window we can upload those
        if fs + window + window < since_epoch {
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
    Ok(())
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
                        ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset | ErrorKind::TimedOut => (),
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
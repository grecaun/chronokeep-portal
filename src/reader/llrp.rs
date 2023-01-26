use std::{net::TcpStream, thread::{self, JoinHandle}, sync::{self, Arc}, io::Read, io::{Write, ErrorKind}};
use std::time::Duration;

use crate::llrp::{self, parameter_types};

pub const DEFAULT_ZEBRA_PORT: u16 = 5084;

pub struct LLRP {
    id: i64,
    nickname: String,
    kind: String,
    ip_address: String,
    port: u16,

    pub socket: sync::Mutex<Option<TcpStream>>,
    pub keepalive: Arc<sync::Mutex<bool>>,
    pub msg_id: Arc<sync::Mutex<u32>>,
}

impl LLRP {
    pub fn new(
        id: i64,
        nickname: String,
        ip_address: String,
        port: u16,
    ) -> LLRP {
        LLRP {
            id,
            kind: String::from(super::READER_KIND_LLRP),
            nickname,
            ip_address,
            port,
            socket: sync::Mutex::new(None),
            keepalive: Arc::new(sync::Mutex::new(true)),
            msg_id: Arc::new(sync::Mutex::new(0)),
        }
    }
}

impl super::Reader for LLRP {
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

    fn set_time(&self) -> Result<(), &'static str> {
        todo!()
    }

    fn get_time(&self) -> Result<(), &'static str> {
        todo!()
    }

    fn connect(&mut self) -> Result<JoinHandle<()>, &'static str> {
        let res = TcpStream::connect(format!("{}:{}", self.ip_address, self.port));
        match res {
            Err(_) => return Err("unable to connect"),
            Ok(tcp_stream) => {
                self.socket = match tcp_stream.try_clone() {
                    Ok(stream) => sync::Mutex::new(Some(stream)),
                    Err(_) => {
                        return Err("error copying stream to thread")
                    }
                };
                let mut t_stream = tcp_stream;
                let t_mutex = self.keepalive.clone();
                let msg_id = self.msg_id.clone();
                let output = thread::spawn(move|| {
                    let buf: &mut [u8; 1024] = &mut [0;1024];
                    match t_stream.set_read_timeout(Some(Duration::from_secs(1))) {
                        Ok(_) => (),
                        Err(e) => {
                            println!("Error setting read timeout. {e}")
                        }
                    }
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
                        match read(&mut t_stream, buf, &msg_id) {
                            Ok(_) => (),
                            Err(e) => {
                                match e.kind() {
                                    ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset => {
                                        break;
                                    }
                                    ErrorKind::TimedOut => (),
                                    _ => println!("Error reading from reader. {e}"),
                                }
                            }
                        }
                    }
                    // finalize what we're doing
                    let fin_id = match msg_id.lock() {
                        Ok(id) => *id,
                        Err(_) => 0,
                    };
                    let close = llrp::requests::close_connection(&fin_id);
                    match t_stream.write(&close) {
                        Ok(_) => {
                            match read(&mut t_stream, buf, &msg_id) {
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

    fn initialize(&self) -> Result<(), &'static str> {
        todo!()
    }

    fn send(&mut self, buf: &[u8]) -> Result<(), &'static str> {
        if let Ok(stream) = self.socket.lock() {
            match &*stream {
                Some(s) => {
                    let mut w_stream = match s.try_clone() {
                        Ok(v) => v,
                        Err(_) => return Err("unable to copy stream")
                    };
                    match w_stream.write(buf) {
                        Ok(_) => {
                        },
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
}

fn read(tcp_stream: &mut TcpStream, buf: &mut [u8;1024], msg_id: &Arc<sync::Mutex<u32>>) -> Result<(), std::io::Error> {
    let numread = tcp_stream.read(buf);
    match numread {
        Ok(num) => {
            if num > 0 {
                let msg_type = llrp::bit_masks::get_msg_type(&buf[0..10]);
                match msg_type {
                    Ok(info) => {
                        let found_type = match llrp::message_types::get_message_name(info.kind) {
                            Some(found) => found,
                            _ => "UNKNOWN",
                        };
                        match info.kind {
                            llrp::message_types::KEEPALIVE => {
                                println!("{} found.", found_type);
                                let response = llrp::requests::keepalive_ack(&info.id);
                                match tcp_stream.write(&response) {
                                    Ok(_) => (),
                                    Err(e) => println!("Error responding to keepalive. {e}"),
                                }
                            },
                            llrp::message_types::GET_ROSPECS_RESPONSE => {
                                println!("{} found.", found_type);
                                process_parameters(&buf, 10, &num);
                            },
                            _ => {
                                println!("Message Type Found! V: {} - {}", info.version, found_type);
                                process_parameters(&buf, 10, &num);
                            },
                        }
                        if let Ok(mut id) = msg_id.lock() {
                            *id = info.id + 1;
                        }
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
    Ok(())
}

fn process_parameters(buf: &[u8;1024], start_ix: usize, num: &usize) {
    let mut start: usize = start_ix;
    while start < *num {
        let bits: u32 = ((buf[start] as u32) << 24) + ((buf[start+1] as u32) << 16) + ((buf[start+2] as u32) << 8) + (buf[start+3] as u32);
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
                let rospec_id: u32 = ((buf[start+4] as u32) << 24) + ((buf[start+5] as u32) << 16) + ((buf[start+6] as u32) << 8) + (buf[start+7] as u32);
                // Valid priorities are 0-7, lower are given higher priority
                let priority: u8 = buf[start+8];
                // 0 = disabled, 1 = inactive, 2 = active
                let current_state: u8 = buf[start+9];
                // 10 is a ROBoundarySpec parameter followed by 1-n SpecParameters followed by 0-1 ROReportSpec parameters
                println!("ROSpec Parameter -- id {} - priority {} - current state {}", rospec_id, priority, current_state);
            },
            _ => {
                println!("Parameter found -- {:?} -- TV? {}", parameter_types::get_parameter_name(param_info.kind), param_info.tv);
            }
        }
        start = start + param_info.length as usize;
    }
}
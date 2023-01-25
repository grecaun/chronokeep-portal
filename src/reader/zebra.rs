use std::{net::TcpStream, thread::{self, JoinHandle}, sync::{self, Arc}, io::Read, io::{Write, ErrorKind}};
use std::time::Duration;

use crate::llrp;

pub const DEFAULT_ZEBRA_PORT: u16 = 5084;

pub struct Zebra {
    id: usize,
    nickname: String,
    kind: String,
    ip_address: String,
    port: u16,

    connected: bool,
    connected_at: String,
    pub keepalive: Arc<sync::Mutex<bool>>,
    pub msg_id: Arc<sync::Mutex<u32>>,
}

impl Zebra {
    pub fn new(
        id: usize,
        nickname: String,
        ip_address: String,
        port: u16
    ) -> Zebra {
        Zebra {
            id,
            kind: String::from(super::READER_KIND_ZEBRA),
            nickname,
            ip_address,
            port,
            connected: false,
            connected_at: String::from(""),
            keepalive: Arc::new(sync::Mutex::new(true)),
            msg_id: Arc::new(sync::Mutex::new(0)),
        }
    }
}

impl super::Reader for Zebra {
    fn id(&self) -> usize {
        self.id
    }
    
    fn nickname(&self) -> &str {
        &self.nickname
    }

    fn kind(&self) -> &str{
        &self.kind
    }

    fn ip_address(&self) -> &str {
        &self.ip_address
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn connected_at(&self) -> &str {
        &self.connected_at
    }

    fn equal(&self, other: &dyn super::Reader) -> bool {
        self.nickname == other.nickname() &&
            self.kind == other.kind() &&
            self.ip_address == other.ip_address() &&
            self.port == other.port()
    }

    fn process_messages(&self) -> Result<(), &'static str> {
        todo!()
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
            Ok(mut tcp_stream) => {
                self.connected = true;
                //self.connected_at = "";
                let t_mutex = self.keepalive.clone();
                let msg_id = self.msg_id.clone();
                let output = thread::spawn(move|| {
                    let buf: &mut [u8; 1024] = &mut [0;1024];
                    match tcp_stream.set_read_timeout(Some(Duration::from_secs(1))) {
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
                        match read(&mut tcp_stream, buf, &msg_id) {
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
                    match tcp_stream.write(&close) {
                        Ok(_) => {
                            match read(&mut tcp_stream, buf, &msg_id) {
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
                                println!("Keepalive message received.");
                                let response = llrp::requests::keepalive_ack(&info.id);
                                match tcp_stream.write(&response) {
                                    Ok(_) => (),
                                    Err(e) => println!("Error responding to keepalive. {e}"),
                                }
                            },
                            _ => {
                                println!("Message Type Found! V: {} - {}", info.version, found_type);
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
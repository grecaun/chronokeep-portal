use std::{net::TcpStream, thread::{self, JoinHandle}, sync, io::Read};

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
    // list of sockets to be connected to
    pub buff: [u8; 1024],
    pub joiner: Option<JoinHandle<()>>,
    pub keepalive: sync::Mutex<bool>,
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
            buff: [0; 1024],
            joiner: None,
            keepalive: sync::Mutex::new(true)
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

    fn connect(&mut self) -> Result<(), &'static str> {
        let res = TcpStream::connect(format!("{}:{}", self.ip_address, self.port));
        match res {
            Err(_) => return Err("unable to connect"),
            Ok(mut tcp_stream) => {
                self.joiner = Some(thread::spawn(move|| {
                    let buf: &mut [u8; 1024] = &mut [0;1024];
                    loop {
                        let numread = tcp_stream.read(buf);
                        match numread {
                            Ok(num) => {
                                if num > 0 {
                                    let bits: u16 = (u16::from(buf[0]) << 8) + u16::from(buf[1]);
                                    let msg_type = llrp::bit_masks::get_msg_type(&bits);
                                    match msg_type {
                                        Ok(info) => {
                                            let found_type = match llrp::message_types::get_message_name(info.kind) {
                                                Some(found) => found,
                                                _ => "UNKNOWN",
                                            };
                                            println!("Message Type Found! V: {} - {}", info.version, found_type);
                                        },
                                        Err(e) => {
                                            println!("Error finding message type: {e}");
                                        },
                                    }
                                }
                            }
                            Err(e) => {
                                println!("Error! {e}");
                                continue
                            },
                        }
                    }
                }));
                Ok(())
            },
        }
    }

    fn initialize(&self) -> Result<(), &'static str> {
        todo!()
    }
}
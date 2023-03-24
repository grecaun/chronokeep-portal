use std::{net::{UdpSocket, Ipv4Addr}, sync::{Arc, Mutex}, time::Duration, io::ErrorKind, str::FromStr};

use rand::{thread_rng, Rng};

use crate::{database::{Database, sqlite}};

use super::SETTING_PORTAL_NAME;

pub const ZERO_CONF_REQUEST: &str = "[DISCOVER_CHRONO_SERVER_REQUEST]";
pub const ZERO_CONF_MULTICAST_ADDR: &str = "224.0.44.88";
pub const ZERO_CONF_PORT: u16 = 4488;

pub struct ZeroConf {
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    server_id: String,
    control_port: u16,
    keepalive: Arc<Mutex<bool>>,
    socket: UdpSocket
}

impl ZeroConf {
    pub fn new(sqlite: Arc<Mutex<sqlite::SQLite>>, control_port: &u16, keepalive: Arc<Mutex<bool>>) -> Result<ZeroConf, &'static str> {
        let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
        let mut server_id = String::from("");
        let mut rng = thread_rng();
        for _ in 0..10 {
            server_id.push(chars[rng.gen_range(0..chars.len())])
        }
        println!("Zero Conf Server id is {}, port is {}", server_id, ZERO_CONF_PORT);
        let socket = match UdpSocket::bind(format!("0.0.0.0:{}", ZERO_CONF_PORT)) {
            Ok(sock) => sock,
            Err(e) => {
                println!("Something went wrong trying to connect to zero conf port: {e}");
                return Err("unable to establish udp socket");
            }
        };
        match socket.set_read_timeout(Some(Duration::new(2,0))) {
            Ok(_) => {},
            Err(e) => {
                println!("Unable to set read timeout on socket: {e}");
                return Err("unable to set read timeout")
            }
        }
        match socket.join_multicast_v4(
            &Ipv4Addr::from_str(ZERO_CONF_MULTICAST_ADDR).unwrap(),
            &Ipv4Addr::UNSPECIFIED
        ) {
            Ok(_) => {
                println!("Successfully joined multicast group.");
            },
            Err(e) => {
                println!("Unable to join multicast group. {e}");
                return Err("unable to join multicast group")
            },
        }
        let control_port = *control_port;
        return Ok(ZeroConf {
            sqlite,
            server_id,
            control_port,
            keepalive,
            socket
        })
    }
    
    pub fn run_loop(&self) {
        let mut buffer = [0; 4096];
        loop {
            if let Ok(ka) = self.keepalive.lock() {
                if *ka == false {
                    break;
                }
            } else {
                break;
            }
            let (amt, src) = match self.socket.recv_from(&mut buffer) {
                Ok(a) => {
                    a
                },
                Err(e) => {
                    match e.kind() {
                        ErrorKind::TimedOut |
                        ErrorKind::WouldBlock => {},
                        _ => {
                            println!("Zero Conf - Error receiving: {e}");
                        }
                    }
                    continue
                }
            };
            match std::str::from_utf8(&buffer[0..amt]) {
                Ok(rcvd) => { 
                    match rcvd {
                        ZERO_CONF_REQUEST => {
                            let mut response = format!("[{}|{}|{}]", "Unknown", self.server_id, self.control_port);
                            if let Ok(sq) = self.sqlite.lock() {
                                match sq.get_setting(SETTING_PORTAL_NAME) {
                                    Ok(name) => {
                                        response = format!("[{}|{}|{}]", name.value(), self.server_id, self.control_port)
                                    }
                                    Err(e) => {
                                        println!("Error getting server name: {e}")
                                    }
                                }
                            }
                            match self.socket.send_to(response.as_bytes(), src) {
                                Ok(num) => {
                                    println!("Sent {response} -- {src} -- {num} bytes.");
                                },
                                Err(e) => {
                                    println!("Error sending response: {e}");
                                }
                            };
                        },
                        u => {
                            println!("Unknown request received: {u}");
                        }
                    };
                },
                Err(e) => {
                    println!("Error translating value received: {e}");
                }
            };
        }
        println!("Zero Conf Server has shut down.");
    }
}
use std::{net::UdpSocket, sync::{Arc, Mutex}, time::Duration, io::ErrorKind};

use rand::{thread_rng, Rng};

use super::Control;

pub const ZERO_CONF_REQUEST: &str = "[DISCOVER_CHRONO_SERVER_REQUEST]";
pub const ZERO_CONF_SERVER_COMPAT: &str = "0010"; // 0010 means control only

pub struct ZeroConf {
    controls: Control,
    server_id: String,
    control_port: u16,
    keepalive: Arc<Mutex<bool>>,
    socket: UdpSocket
}

impl ZeroConf {
    pub fn new(controls: Control, control_port: &u16, keepalive: Arc<Mutex<bool>>) -> Result<ZeroConf, &'static str> {
        let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
        let mut server_id = String::from("");
        let mut rng = thread_rng();
        for _ in 0..10 {
            server_id.push(chars[rng.gen_range(0..chars.len())])
        }
        println!("Server id is {}, port is {}", server_id, controls.zero_conf_port);
        let socket = match UdpSocket::bind(format!("0.0.0.0:{}", controls.zero_conf_port)) {
            Ok(sock) => sock,
            Err(e) => {
                println!("Something went wrong trying to connect to zero conf port: {e}");
                return Err("unable to establish udp socket");
            }
        };
        match socket.set_read_timeout(Some(Duration::new(5,0))) {
            Ok(_) => {},
            Err(e) => {
                println!("Unable to set read timeout on socket: {e}");
                return Err("unable to set read timeout")
            }
        }
        let control_port = *control_port;
        return Ok(ZeroConf {
            controls,
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
                        ErrorKind::TimedOut => {},
                        _ => {
                            println!("Error receiving: {e}");
                        }
                    }
                    continue
                }
            };
            match std::str::from_utf8(&buffer[0..amt]) {
                Ok(rcvd) => { 
                    match rcvd {
                        ZERO_CONF_REQUEST => {
                            let response = format!("[{}|{}|{}|{}]", self.controls.name, self.server_id, self.control_port, ZERO_CONF_SERVER_COMPAT);
                            match self.socket.send_to(response.as_bytes(), src) {
                                Ok(num) => {
                                    println!("Sent {response} -- {num} bytes.");
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
    }
}
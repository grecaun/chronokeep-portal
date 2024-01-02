use std::{net::{UdpSocket, Ipv4Addr, SocketAddr}, sync::{Arc, Mutex}, time::Duration, io::ErrorKind, str::FromStr};

use rand::{thread_rng, Rng};
use socket2::{Socket, Domain, Type, Protocol};

use crate::database::{Database, sqlite};

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
        let socket = match Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
            Ok(sock) => sock,
            Err(e) => {
                println!("Something went wrong trying to create our socket: {e}");
                return Err("unable to create udp socket")
            }
        };
        let address: SocketAddr = match format!("0.0.0.0:{ZERO_CONF_PORT}").parse() {
            Ok(a) => a,
            Err(e) => {
                println!("Error creating SockAddr: {e}");
                return Err("unable to create sock address")
            }
        };
        let address = address.into();
        // on windows specifically, SO_REUSEADDR must be set before bind or
        // it does not work
        match socket.set_reuse_address(true) {
            Ok(_) => {}
            Err(e) => {
                println!("Unable to set SO_REUSEADDR to true: {e}");
                return Err("error setting SO_REUSEADDR to true")
            }
        }
        match socket.bind(&address) {
            Ok(_) => {
                println!("Zero conf socket successfully bound.");
            }
            Err(e) => {
                println!("Error binding zero conf socket: {e}");
                return Err("error binding socket")
            }
        }
        let socket: UdpSocket = socket.into();
        match socket.set_read_timeout(Some(Duration::new(2,0))) {
            Ok(_) => {},
            Err(e) => {
                println!("Unable to set read timeout on socket: {e}");
                return Err("unable to set read timeout")
            }
        }
        // With multiple interfaces, we should join the multicast on all
        let addresses = match if_addrs::get_if_addrs() {
            Ok(addrs) => addrs,
            Err(e) => {
                println!("Error getting network interfaces: {e}");
                return Err("error getting network interfaces")
            }
        };
        for iface in addresses {
            if let if_addrs::IfAddr::V4(addr) = iface.addr {
                if addr.is_loopback() == false {
                    match socket.join_multicast_v4(
                        &Ipv4Addr::from_str(ZERO_CONF_MULTICAST_ADDR).unwrap(),
                        &addr.ip
                    ) {
                        Ok(_) => {
                            println!("Successfully joined multicast group on ip {}.", addr.ip);
                        },
                        Err(e) => {
                            println!("Unable to join multicast group. {e}");
                            return Err("unable to join multicast group")
                        },
                    }
                }
            }
        }
        let control_port = *control_port;
        Ok(ZeroConf {
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
        // Leaving multicast groups, do the opposite of the join
        match if_addrs::get_if_addrs() {
            Ok(addrs) => {
                for iface in addrs {
                    if let if_addrs::IfAddr::V4(addr) = iface.addr {
                        if addr.is_loopback() == false {
                            match self.socket.leave_multicast_v4(
                                &Ipv4Addr::from_str(ZERO_CONF_MULTICAST_ADDR).unwrap(),
                                &addr.ip
                            ) {
                                Ok(_) => {
                                    println!("Successfully left multicast group on ip {}.", addr.ip);
                                },
                                Err(e) => {
                                    println!("Unable to leave multicast group. {e}");
                                },
                            }
                        }
                    }
                }
            },
            Err(e) => {
                println!("Error getting network interfaces: {e}");
            }
        };
        println!("Zero Conf Server has shut down.");
    }
}
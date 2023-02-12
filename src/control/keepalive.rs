use std::{sync::{Arc, Mutex}, net::TcpStream, thread, time::Duration};

use super::socket::{MAX_CONNECTED, write_keepalive};

pub const KEEPALIVE_INTERVAL_SECONDS: u64 = 30;

pub struct KeepAlive {
    control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED +1]>>,
    keepalive: Arc<Mutex<bool>>
}

impl KeepAlive {
    pub fn new(
        control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED +1]>>,
        keepalive: Arc<Mutex<bool>>
    ) -> KeepAlive {
        return KeepAlive { control_sockets, keepalive }
    }

    pub fn run_loop(&self) {
        loop {
            if let Ok(ka) = self.keepalive.lock() {
                if *ka == false {
                    break;
                }
            } else {
                println!("Error getting keep alive mutex. Exiting.");
                break;
            }
            if let Ok(c_socks) = self.control_sockets.lock() {
                for sock in c_socks.iter() {
                    if let Some(sock) = sock {
                        match write_keepalive(&sock) {
                            Ok(_) => (),
                            Err(_) => {
                                println!("error writing to socket")
                                // TODO break and connection probably dead, should close
                            }
                        }
                    }
                }
            }
            thread::sleep(Duration::from_secs(KEEPALIVE_INTERVAL_SECONDS))
        }
    }
}
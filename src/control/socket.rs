use std::{str, thread::{JoinHandle, self}, sync::{Mutex, Arc}, net::{TcpListener, TcpStream, Shutdown}, io::Read};

use crate::{database::sqlite, reader};

pub mod requests;

pub fn control_loop(_sqlite: Arc<Mutex<sqlite::SQLite>>, controls: super::Control) {
    let keepalive: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));
    let mut _input: String = String::new();
    let mut _connected: Vec<Box<dyn reader::Reader>> = Vec::new();
    let mut _joiners: Vec<JoinHandle<()>> = Vec::new();

    let listener = match TcpListener::bind(format!("127.0.0.1:{}", controls.control_port)) {
        Ok(list) => list,
        Err(e) => {
            println!("Error opening listener. {e}");
            return
        }
    };

    loop {
        if let Ok(ka) = keepalive.lock() {
            if *ka == false {
                break;
            }
        } else {
            println!("Error getting keep alive mutex. Exiting.");
            break;
        }
        match listener.accept() {
            Ok((stream, addr)) => {
                println!("New connection: {}", addr);
                let t_keepalive = keepalive.clone();
                let t_port = controls.control_port.clone();
                thread::spawn(move|| {
                    handle_stream(stream, t_keepalive, t_port);
                });
            },
            Err(e) => {
                println!("Connection failed. {e}")
            }
        }
    }
}

fn handle_stream(mut stream: TcpStream, keepalive: Arc<Mutex<bool>>, port: u16) {
    let mut data = [0 as u8; 51200];
    loop {
        if let Ok(ka) = keepalive.lock() {
            if *ka == false {
                break;
            }
        } else {
            println!("Error getting keep alive mutex. Exiting.");
            break;
        }
        let size = match stream.read(&mut data) {
            Ok(size) => size,
            Err(e) => {
                println!("Error reading from socket. {e}");
                stream.shutdown(Shutdown::Both).unwrap();
                break;
            },
        };
        if size > 0 {
            let cmd: requests::Request = match serde_json::from_slice(&data[0..size]) {
                Ok(data) => data,
                Err(e) => {
                    println!("Error deserializing request. {e}");
                    requests::Request::Unknown
                },
            };
            match cmd {
                requests::Request::Quit => {
                    if let Ok(mut ka) = keepalive.lock() {
                        *ka = false;
                    }
                    _ = TcpStream::connect(format!("127.0.0.1:{port}"));
                },
                unknown => {
                    println!("Request {:?} not yet implemented.", unknown);
                }
            }
        }
    }
}
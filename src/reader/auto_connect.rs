use std::{sync::{Arc, Mutex}, thread::{JoinHandle, self}, net::TcpStream, time::Duration};

use serde::{Serialize, Deserialize};

use crate::{control::{socket::{MAX_CONNECTED, self}, self}, database::sqlite, reader::AUTO_CONNECT_TRUE, processor};

pub const START_UP_WAITING_PERIOD_SECONDS: u64 = 30;

#[derive(Serialize, Deserialize, Clone)]
pub enum State {
    Waiting,
    Running,
    Finished,
    Unknown,
}

pub struct AutoConnector {
    state: Arc<Mutex<State>>,
    readers: Arc<Mutex<Vec<super::Reader>>>,
    joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
    control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
    read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
    sight_processor: Arc<processor::SightingsProcessor>,
    controls: control::Control,
    sqlite: Arc<Mutex<sqlite::SQLite>>
}

impl AutoConnector {
    pub fn new(
        state: Arc<Mutex<State>>,
        readers: Arc<Mutex<Vec<super::Reader>>>,
        joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
        control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
        read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
        sight_processor: Arc<processor::SightingsProcessor>,
        controls: control::Control,
        sqlite: Arc<Mutex<sqlite::SQLite>>
    ) -> AutoConnector {
        AutoConnector {
            state,
            readers,
            joiners,
            control_sockets,
            read_repeaters,
            sight_processor,
            controls,
            sqlite
        }
    }

    pub fn get_state(&self) -> State {
        let mut output = State::Unknown;
        if let Ok(state) = self.state.lock() {
            output = state.clone();
        }
        output
    }

    pub fn run(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            *state = State::Waiting;
        } else {
            println!("Error getting state mutex during reader auto start sequence.");
            return
        }
        println!("Auto connect is pausing for {START_UP_WAITING_PERIOD_SECONDS} seconds before trying to connect to readers.");
        thread::sleep(Duration::from_secs(START_UP_WAITING_PERIOD_SECONDS));
        if let Ok(mut state) = self.state.lock() {
            *state = State::Running;
        } else {
            println!("Error getting state mutex during reader auto start sequence.");
            return
        }
        println!("Auto connect is done waiting. Connecting now.");
        if let Ok(mut readers) = self.readers.lock() {
            for reader in readers.iter_mut() {
                if reader.auto_connect() == AUTO_CONNECT_TRUE {
                    println!("Connecting to reader {}.", reader.nickname());
                    reader.set_control_sockets(self.control_sockets.clone());
                    reader.set_read_repeaters(self.read_repeaters.clone());
                    reader.set_sight_processor(self.sight_processor.clone());
                    match reader.connect(&self.sqlite, &self.controls) {
                        Ok(j) => {
                            if let Ok(mut join) = self.joiners.lock() {
                                join.push(j);
                            }
                            println!("Initializing reader.");
                            match reader.initialize() {
                                Ok(_) => {},
                                Err(e) => {
                                    println!("Error initializing reader: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            println!("Error connecting to reader: {e}");
                        }
                    }
                }
            }
            println!("Sending reader updates to connected sockets.");
            if let Ok(c_socks) = self.control_sockets.lock() {
                for sock in c_socks.iter() {
                    if let Some(sock) = sock {
                        _ = socket::write_reader_list(&sock, &readers);
                    }
                }
            }
        }
        println!("All done connecting to readers.");
        if let Ok(mut state) = self.state.lock() {
            *state = State::Finished
        }
    }
}
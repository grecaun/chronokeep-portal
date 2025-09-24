use std::{net::TcpStream, sync::{Arc, Mutex}, thread::{self, JoinHandle}, time::{Duration, SystemTime}};

use chrono::{DateTime, Local};
use serde::{Serialize, Deserialize};

use crate::{control::{self, socket::MAX_CONNECTED, sound::{SoundNotifier, SoundType}}, database::sqlite, notifier, processor, reader::{reconnector::Reconnector, AUTO_CONNECT_TRUE}};

pub const START_UP_WAITING_PERIOD_SECONDS: u64 = 60;

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
    control: Arc<Mutex<control::Control>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    read_saver: Arc<processor::ReadSaver>,
    sound: Arc<SoundNotifier>,
    notifier: notifier::Notifier,
}

impl AutoConnector {
    pub fn new(
        state: Arc<Mutex<State>>,
        readers: Arc<Mutex<Vec<super::Reader>>>,
        joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
        control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
        read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
        control: Arc<Mutex<control::Control>>,
        sqlite: Arc<Mutex<sqlite::SQLite>>,
        read_saver: Arc<processor::ReadSaver>,
        sound: Arc<SoundNotifier>,
        notifier: notifier::Notifier,
    ) -> AutoConnector {
        AutoConnector {
            state,
            readers,
            joiners,
            control_sockets,
            read_repeaters,
            control,
            sqlite,
            read_saver,
            sound,
            notifier,
        }
    }

    pub fn get_state(&self) -> State {
        let mut output = State::Unknown;
        if let Ok(state) = self.state.lock() {
            output = state.clone();
        }
        output
    }

    pub fn run(&mut self, quick: bool) {
        if let Ok(mut state) = self.state.lock() {
            *state = State::Waiting;
        } else {
            println!("Error getting state mutex during reader auto start sequence.");
            return
        }
        if !quick {
            println!("Auto connect is pausing for {START_UP_WAITING_PERIOD_SECONDS} seconds before trying to connect to readers.");
            thread::sleep(Duration::from_secs(START_UP_WAITING_PERIOD_SECONDS));
        } else {
            println!("Auto connect is in quick mode. Wait period shortened to 1 second.");
            thread::sleep(Duration::from_secs(1));
        }
        if let Ok(mut state) = self.state.lock() {
            *state = State::Running;
        } else {
            println!("Error getting state mutex during reader auto start sequence.");
            return
        }
        println!("Auto connect is done waiting. Connecting now.");
        let mut unable_to_connect = false;
        if let Ok(mut readers) = self.readers.lock() {
            for reader in readers.iter_mut() {
                if reader.auto_connect() == AUTO_CONNECT_TRUE {
                    println!("Connecting to reader {}.", reader.nickname());
                    reader.set_control_sockets(self.control_sockets.clone());
                    reader.set_readers(self.readers.clone());
                    reader.set_read_repeaters(self.read_repeaters.clone());
                    let reconnector = Reconnector::new(
                        self.readers.clone(),
                        self.joiners.clone(),
                        self.control_sockets.clone(),
                        self.read_repeaters.clone(),
                        self.control.clone(),
                        self.sqlite.clone(),
                        self.read_saver.clone(),
                        self.sound.clone(),
                        reader.id(),
                        1,
                        self.notifier.clone(),
                    );
                    match reader.connect(
                        &self.sqlite.clone(),
                        &self.control.clone(),
                        &self.read_saver.clone(),
                        self.sound.clone(),
                        Some(reconnector),
                        self.notifier.clone(),
                    ) {
                        Ok(j) => {
                            if let Ok(mut join) = self.joiners.lock() {
                                join.push(j);
                            }
                        }
                        Err(e) => {
                            println!("Error connecting to reader: {e}");
                            unable_to_connect = true;
                        }
                    }
                }
            }
        }
        println!("All done connecting to readers.");
        if unable_to_connect {
            let date_time: DateTime<Local> = SystemTime::now().into();
            self.notifier.send_notification(notifier::Notification::UnableToStartReading, format!("{}", date_time.format("%Y/%m/%d %T")));
        } else {
            let date_time: DateTime<Local> = SystemTime::now().into();
            self.notifier.send_notification(notifier::Notification::StartReading, format!("{}", date_time.format("%Y/%m/%d %T")));
        }
        self.sound.notify_custom(SoundType::StartupFinished);
        if let Ok(mut state) = self.state.lock() {
            *state = State::Finished
        }
    }
}
/*
Chronokeep Portal - RFID Tag Reading Software
Copyright (C) 2026 James Sentinella

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{sync::{Arc, Mutex}, thread::{self, JoinHandle}, time::{Duration, SystemTime}};

use chrono::{DateTime, Local};
use serde::{Serialize, Deserialize};

use crate::{control::{self, socket::MAX_CONNECTED, sound::{SoundNotifier, SoundType}}, database::sqlite, notifier, processor, reader::{AUTO_CONNECT_TRUE, reconnector::Reconnector}, remote::uploader};

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
    control_sockets: crate::control::client::ControlSockets,
    read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
    control: Arc<Mutex<control::Control>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    uploader: Arc<uploader::Uploader>,
    read_saver: Arc<processor::ReadSaver>,
    sound: Arc<SoundNotifier>,
    notifier: notifier::Notifier,
}

impl AutoConnector {
    pub fn new(
        state: Arc<Mutex<State>>,
        readers: Arc<Mutex<Vec<super::Reader>>>,
        joiners: Arc<Mutex<Vec<JoinHandle<()>>>>,
        control_sockets: crate::control::client::ControlSockets,
        read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
        control: Arc<Mutex<control::Control>>,
        sqlite: Arc<Mutex<sqlite::SQLite>>,
        read_saver: Arc<processor::ReadSaver>,
        sound: Arc<SoundNotifier>,
        notifier: notifier::Notifier,
        uploader: Arc<uploader::Uploader>,
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
            uploader,
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
        let uptime = system_uptime();
        let wait = startup_wait(quick, uptime);
        match uptime {
            Some(up) => println!("System has been up for {} seconds. Auto connect is pausing for {} seconds before trying to connect to readers.", up.as_secs(), wait.as_secs()),
            None => println!("Auto connect is pausing for {} seconds before trying to connect to readers.", wait.as_secs()),
        }
        thread::sleep(wait);
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
                    reader.set_uploader(self.uploader.clone());
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
                        self.uploader.clone(),
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

/// Minimum pause before connecting, even when the readers have had time to start.
const MINIMUM_WAIT: Duration = Duration::from_secs(1);

/// How long to wait before connecting to readers.
///
/// Readers need about a minute after power-on before they accept connections, and they're
/// powered on with the computer. When the system uptime is known, only the part of that minute
/// that hasn't passed yet is waited out, so restarting portal right after boot can't skip it.
/// When it isn't known, `quick` shortens the wait.
fn startup_wait(quick: bool, uptime: Option<Duration>) -> Duration {
    let full = Duration::from_secs(START_UP_WAITING_PERIOD_SECONDS);
    match uptime {
        Some(up) => full.saturating_sub(up).max(MINIMUM_WAIT),
        None if quick => MINIMUM_WAIT,
        None => full,
    }
}

/// Time since the computer booted, from /proc/uptime. None where that isn't available.
fn system_uptime() -> Option<Duration> {
    let contents = std::fs::read_to_string("/proc/uptime").ok()?;
    let seconds: f64 = contents.split_whitespace().next()?.parse().ok()?;
    Duration::try_from_secs_f64(seconds).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: Duration = Duration::from_secs(START_UP_WAITING_PERIOD_SECONDS);

    #[test]
    fn waits_full_period_right_after_boot() {
        assert_eq!(startup_wait(false, Some(Duration::ZERO)), FULL);
    }

    #[test]
    fn quick_flag_cannot_skip_remaining_wait_after_boot() {
        assert_eq!(startup_wait(true, Some(Duration::from_secs(10))), FULL - Duration::from_secs(10));
    }

    #[test]
    fn restart_long_after_boot_waits_minimum() {
        assert_eq!(startup_wait(false, Some(Duration::from_secs(3600))), MINIMUM_WAIT);
        assert_eq!(startup_wait(false, Some(FULL)), MINIMUM_WAIT);
    }

    #[test]
    fn unknown_uptime_uses_quick_flag() {
        assert_eq!(startup_wait(false, None), FULL);
        assert_eq!(startup_wait(true, None), MINIMUM_WAIT);
    }
}

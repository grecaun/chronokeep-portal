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

use std::{sync::{Arc, Condvar, Mutex, WaitTimeoutResult}, time::Duration};

use crate::{control, database::{Database, sqlite}, network::{api, heartbeat::HeartbeatState}, reader::{self, Reader}, remote::uploader::info::UploadInfo};

#[derive(Clone)]
pub struct Heartbeat {
    start_time: i64,
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<control::Control>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    waiter: Arc<(Mutex<bool>, Condvar)>,
    readers: Arc<Mutex<Vec<Reader>>>,
    upload_info: Arc<Mutex<UploadInfo>>
}

impl Heartbeat {
    pub fn new(
        control: Arc<Mutex<control::Control>>,
        sqlite: Arc<Mutex<sqlite::SQLite>>,
        readers: Arc<Mutex<Vec<Reader>>>,
        upload_info: Arc<Mutex<UploadInfo>>
    ) -> Self {
        Heartbeat {
            start_time: chrono::Utc::now().timestamp_millis(),
            keepalive: Arc::new(Mutex::new(false)),
            waiter: Arc::new((Mutex::new(true), Condvar::new())),
            control,
            sqlite,
            readers,
            upload_info
        }
    }

    pub fn stop(&self) {
        if let Ok(mut keepalive_guard) = self.keepalive.lock() {
            *keepalive_guard = false;
        }
        let (lock, cvar) = &*self.waiter;
        let mut waiting = lock.lock().unwrap();
        *waiting = false;
        cvar.notify_one();
        drop(waiting);
    }

    pub fn running(&self) -> bool {
        if let Ok(keepalive_guard) = self.keepalive.lock() {
            *keepalive_guard
        } else {
            false
        }
    }

    pub fn run(&self) {
        if let Ok(mut keepalive_guard) = self.keepalive.lock() {
            *keepalive_guard = true;
        }
        loop {
            let now = chrono::Utc::now().timestamp_millis();
            let uptime = (now - self.start_time) / 1000; // Uptime in seconds

            let mut dev = String::new();
            let mut bat = 0; // Initialize battery level
            let mut voltage = String::new(); // Initialize battery voltage
            let mut chip_type = String::new();
            if let Ok(control_guard) = self.control.try_lock() {
                bat = control_guard.battery;
                dev = control_guard.name.clone();
                voltage = control_guard.battery_voltage.clone();
                chip_type = control_guard.chip_type.clone();
            }
            if bat > 100 {
                bat = 100;
            }

            let mut total = 0;
            let mut connected = 0;
            let mut reading = false;
            if let Ok(readers) = self.readers.lock() {
                for read in readers.iter() {
                    total += 1;
                    if let Some(true) = read.is_connected() {
                        connected += 1;
                    }
                    if let Ok(ant) = read.antennas.lock() {
                        for ix in 0..16 {
                            if ant[ix] == reader::ANTENNA_STATUS_CONNECTED {
                                reading = true;
                            }
                        }
                    }
                }
            }

            let mut up_err = 0;
            if let Ok(up_info) = self.upload_info.lock() {
                up_err = up_info.total_errors;
            }

            let _heartbeat_state = HeartbeatState {
                dev: dev.clone(),
                location: dev.clone(),
                now,
                uptime,
                battery: voltage,
                charging: bat >= 110,
                hash: format!("Chronokeep Portal v{}", env!("CARGO_PKG_VERSION")),
                upload_errors: up_err,
                readers_connected: connected,
                readers_total: total,
                reading,
                chip_type,
            };

            // Send heartbeat state to the api servers
            if let Ok(sq) = self.sqlite.lock() {
                match sq.get_apis() {
                    Ok(apis) => {
                        for api in &apis {
                            if (api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF) && api.heartbeat() {
                                // send heartbeat_state
                            }
                        }
                    },
                    Err(e) => {
                        println!("Error getting all APIS: {e}");
                    }
                }
            }

            // Send a keepalive signal every 60 seconds, and check if we should continue running
            let (lock, cvar) = &*self.waiter.clone();
            let mut waiting = lock.lock().unwrap();
            let mut result: WaitTimeoutResult;
            while *waiting {
                (waiting, result) = cvar.wait_timeout(waiting, Duration::from_secs(60)).unwrap();
                if result.timed_out() {
                    break;
                }
            }
            *waiting = true;
            drop(waiting);
        }
    }
}
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

use std::{sync::{Arc, Mutex}, thread, time::Duration};

use crate::{control, network::heartbeat::HeartbeatState};

#[derive(Clone)]
pub struct Heartbeat {
    start_time: i64,
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<control::Control>>,
}

impl Heartbeat {
    pub fn new(control: Arc<Mutex<control::Control>>) -> Self {
        Heartbeat {
            start_time: chrono::Utc::now().timestamp_millis(),
            keepalive: Arc::new(Mutex::new(false)),
            control,
        }
    }

    pub fn stop(&self) {
        if let Ok(mut keepalive_guard) = self.keepalive.lock() {
            *keepalive_guard = false;
        }
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
        'outer: loop {
            let now = chrono::Utc::now().timestamp_millis();
            let uptime = (now - self.start_time) / 1000; // Uptime in seconds

            let mut dev = String::new();
            let mut bat = 0; // Initialize battery level
            let mut voltage = String::new(); // Initialize battery voltage
            if let Ok(control_guard) = self.control.try_lock() {
                bat = control_guard.battery;
                dev = control_guard.name.clone();
                voltage = control_guard.battery_voltage.clone();
            }
            if bat > 100 {
                bat = 100;
            }

            let _heartbeat_state = HeartbeatState {
                dev: dev.clone(),
                location: dev.clone(),
                now,
                uptime,
                battery: voltage,
                battery_pct: bat.to_string(),
                charging: bat >= 110,
                hash: format!("Chronokeep Portal v{}", env!("CARGO_PKG_VERSION")),
                upload_errors: 0,
                readers_connected: 0,
                readers_total: 0,
                reading: false,
                chip_type: String::new(),
            };

            // Send heartbeat state to the server

            // Send a keepalive signal every 60 seconds, and check if we should continue running
            for _ in 0..12 {
                thread::sleep(Duration::from_secs(5));
                if let Ok(keepalive_guard) = self.keepalive.try_lock() {
                    if *keepalive_guard == false {
                        drop(keepalive_guard);
                        break 'outer;
                    }
                }
            }
        }
    }
}
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

use std::{collections::HashSet, sync::{Arc, Mutex}, thread, time::Duration};

use crate::{control, network::heartbeat::HeartbeatState, objects::read};

#[derive(Clone)]
pub struct Heartbeat {
    start_time: i64,
    tags: Arc<Mutex<TagCounts>>,
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<control::Control>>,
}

#[derive(Debug, Clone)]
pub struct TagCounts {
    total_count: u64,
    unique_tags: HashSet<String>,
}

impl Heartbeat {
    pub fn new(control: Arc<Mutex<control::Control>>) -> Self {
        Heartbeat {
            start_time: chrono::Utc::now().timestamp_millis(),
            tags: Arc::new(Mutex::new(TagCounts {
                total_count: 0,
                unique_tags: HashSet::new(),
            })),
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

    pub fn update(&mut self, new_reads: Vec<read::Read>) {
        if let Ok(mut guard) = self.tags.lock() {
            guard.total_count += new_reads.len() as u64;
            for read in new_reads {
                guard.unique_tags.insert(read.chip().to_string());
            }   
        }
    }

    pub fn run(&self) {
        if let Ok(mut keepalive_guard) = self.keepalive.lock() {
            *keepalive_guard = true;
        }
        'outer: loop {
            let now = chrono::Utc::now().timestamp_millis();
            let uptime = now - self.start_time;
            let tags = if let Ok(guard) = self.tags.lock() {
                (guard.total_count, guard.unique_tags.len() as u64)
            } else {
                (0, 0)
            };

            let mut dev = String::new();
            let mut bat = 0; // Initialize battery level
            if let Ok(control_guard) = self.control.try_lock() {
                bat = control_guard.battery;
                dev = control_guard.name.clone();
            }
            if bat > 100 {
                bat = 100;
            }

            let _heartbeat_state = HeartbeatState {
                epoch: now.to_string(),
                now,
                uptime: uptime.to_string(),
                dev: dev.clone(),
                evid: "0".to_string(),
                total_count: tags.0,
                unique_count: tags.1,
                battery: bat,
            };

            // Send heartbeat state to the server

            // Send a keepalive signal every 60 seconds, and check if we should continue running
            for _ in 0..5 {
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
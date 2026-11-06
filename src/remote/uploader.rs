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

use std::{collections::HashMap, sync::{atomic::{AtomicU64, Ordering}, Arc, Mutex}, time::{Duration, Instant}};

use reqwest::StatusCode;
use serde::Serialize;

use crate::{control::{Control, socket::write_uploader_status}, database::{Database, sqlite}, defaults, network::api, objects::read, remote::uploader::info::UploadInfo};
use crate::remote::remote_util;

pub mod info;

/// Upload as soon as this many new reads have come in, without waiting for the
/// upload interval.
pub const UPLOAD_READ_THRESHOLD: usize = 25;

/// What the upload loop is waiting on between uploads.
#[derive(Default)]
struct WaitState {
    /// New reads since the last upload started.
    new_reads: usize,
    /// Set to stop waiting right away (e.g. when stopping).
    wake: bool,
}

type Waiter = (Mutex<WaitState>, std::sync::Condvar);

fn add_new_reads(waiter: &Waiter, count: usize) {
    let (lock, cvar) = waiter;
    if let Ok(mut state) = lock.lock() {
        state.new_reads += count;
        if state.new_reads >= UPLOAD_READ_THRESHOLD {
            cvar.notify_one();
        }
    }
}

/// Waits until `interval` is up, UPLOAD_READ_THRESHOLD new reads have come in,
/// or the uploader is woken (e.g. to stop), whichever comes first.
fn wait_for_next_upload(waiter: &Waiter, interval: Duration) {
    let deadline = Instant::now() + interval;
    let (lock, cvar) = waiter;
    if let Ok(mut state) = lock.lock() {
        while !state.wake && state.new_reads < UPLOAD_READ_THRESHOLD {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            state = match cvar.wait_timeout(state, left) {
                Ok((s, _)) => s,
                Err(_) => break,
            };
        }
    }
}

#[cfg(test)]
mod tests;

#[derive(Clone, PartialEq, Serialize, Debug)]
pub enum Status {
    Running,
    Stopping,
    Stopped,
    Unknown
}

pub struct Uploader {
    server_keepalive: Arc<Mutex<bool>>,
    /// Number of the run that should be going; stop() and each new run change
    /// it, so an older run knows to finish.
    generation: Arc<AtomicU64>,
    /// Held by the run in progress, so a new run waits for the old one to finish.
    run_lock: Arc<Mutex<()>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    status: Arc<Mutex<Status>>,
    control_sockets: crate::control::client::ControlSockets,
    control: Arc<Mutex<Control>>,
    waiter: Arc<Waiter>,
    info: Arc<Mutex<UploadInfo>>,
}

impl Uploader {
    pub fn new(
        keepalive: Arc<Mutex<bool>>,
        sqlite: Arc<Mutex<sqlite::SQLite>>,
        control_sockets: crate::control::client::ControlSockets,
        control: Arc<Mutex<Control>>,
        info: Arc<Mutex<UploadInfo>>,
    ) -> Uploader {
        Uploader {
            server_keepalive: keepalive,
            generation: Arc::new(AtomicU64::new(0)),
            run_lock: Arc::new(Mutex::new(())),
            sqlite,
            status: Arc::new(Mutex::new(Status::Stopped)),
            control_sockets,
            control,
            waiter: Arc::new((Mutex::new(WaitState::default()), std::sync::Condvar::new())),
            info,
        }
    }

    pub fn status(&self) -> Status {
        let mut output = Status::Unknown;
        if let Ok(stat) = self.status.lock() {
            output = stat.clone();
        }
        output
    }

    pub fn running(&self) -> bool {
        let mut output = false;
        if let Ok(r) = self.status.lock() {
            output = Status::Running == *r
        }
        output
    }

    pub fn stop(&self) {
        // Ends the current run (and any run still waiting to start).
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut r) = self.status.lock() {
            *r = Status::Stopping
        }
        // let everyone know we're stopping
        self.update_control_socks(0);
        // stop waiting for the upload interval so the loop can exit
        self.wake();
    }

    /// Wakes the upload loop now instead of at the end of the upload interval.
    pub fn wake(&self) {
        let (lock, cvar) = &*self.waiter;
        if let Ok(mut state) = lock.lock() {
            state.wake = true;
            cvar.notify_one();
        }
    }

    /// Tells the uploader new reads came in. It uploads once UPLOAD_READ_THRESHOLD
    /// have arrived, or when the upload interval is up, whichever comes first.
    pub fn notify_reads(&self, count: usize) {
        add_new_reads(&self.waiter, count);
    }

    pub fn run(&self) {
        let mut err_count: usize = 0;
        // This run replaces any earlier one: that one sees the change and finishes,
        // and this one waits for it, so only one ever uploads.
        let my_run = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.wake();
        let _running = match self.run_lock.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        if self.generation.load(Ordering::SeqCst) != my_run {
            // Stopped, or replaced by a newer run, while waiting. If it was a stop,
            // the earlier run may already be gone, so finish the stop here (a newer
            // run sets Running itself).
            let mut stopped = false;
            if let Ok(mut r) = self.status.lock() {
                if *r == Status::Stopping {
                    *r = Status::Stopped;
                    stopped = true;
                }
            }
            if stopped {
                self.update_control_socks(0);
            }
            return;
        }
        if let Ok(mut r) = self.status.lock() {
            *r = Status::Running;
        }
        // let everyone know we're running
        self.update_control_socks(err_count);
        let http_client: reqwest::blocking::Client;
        match reqwest::blocking::ClientBuilder::new().timeout(Duration::from_secs(5))
                                    .connect_timeout(Duration::from_secs(5)).build() {
            Ok(client) => {
                http_client = client;
            },
            Err(_) => {
                if let Ok(mut r) = self.status.lock() {
                    *r = Status::Stopped;
                    // let everyone know we're stopped
                }
                self.update_control_socks(0);
                println!("Unable to get our http client. Auto upload terminating.");
                return;
            },
        }
        // work loop
        loop {
            // exit our loop if this run was stopped or replaced by a newer one
            if self.generation.load(Ordering::SeqCst) != my_run {
                break;
            }
            // exit our loop and terminate if server is shutting down
            if let Ok(ka) = self.server_keepalive.lock() {
                if *ka == false {
                    break;
                }
            } else {
                println!("Unable to grab server keep alive mutex. Exiting.");
                break;
            }
            // Reads that arrive from here on count toward the next upload.
            if let Ok(mut state) = self.waiter.0.lock() {
                state.new_reads = 0;
                state.wake = false;
            }
            let mut to_upload: Vec<read::Read> = Vec::new();
            let mut upload_apis: Vec<api::Api> = Vec::new();
            // get our reads and then upload them
            // check error count
            // if there are 2 or more errors then don't try to grab the database mutex, instead check net first
            // if the chronokeep results api health endpoint isn't reachable then the rest of the internet
            // is probably not reachable
            // one error could just be random, two means it's happened multiple times in a row
            // and a check to the health endpoint is a quick way to save some work
            let mut attempt_upload = true;
            if err_count > 1 {
                attempt_upload = false;
                match http_client.get("https://api.chronokeep.com/health").send() {
                    Ok(resp) => {
                        match resp.status() {
                            StatusCode::OK | StatusCode::NO_CONTENT => {
                                attempt_upload = true;
                            }
                            _ => {
                                err_count += 1;
                            }
                        }
                    },
                    Err(_) => {
                        err_count += 1;
                    }
                }
                self.update_control_socks(err_count);
            }
            if attempt_upload {
                if let Ok(sq) = self.sqlite.lock() {
                    match sq.get_apis() {
                        Ok(apis) => {
                            let mut found = false;
                            for api in apis {
                                if api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF {
                                    found = true;
                                    upload_apis.push(api.clone());
                                }
                            }
                            if found == false {
                                println!("No remote API set up.");
                                break;
                            }
                            match sq.get_not_uploaded_reads() {
                                Ok(mut reads) => {
                                    to_upload.append(&mut reads);
                                },
                                Err(e) => {
                                    println!("Error getting reads to upload: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            println!("Unable to get apis: {e}");
                        }
                    }
                }
                // upload any reads we found in the database if we found our remote API(s)
                let mut mod_reads: HashMap<i64, read::Read> = HashMap::new();
                let mut mod_read_count: HashMap<i64, usize> = HashMap::new();
                for api in &upload_apis {
                    if to_upload.len() > 0 && api.enabled() {
                        println!("Uploading {} reads to {} at {}", to_upload.len(), api.nickname(), api.uri());
                        let (modified_reads, e_count) = remote_util::upload_all_reads(&http_client, &api, to_upload.clone());
                        println!("Uploaded {} reads to {} at {}. Error count: {}", modified_reads.len(), api.nickname(), api.uri(), e_count);
                        for read in modified_reads.iter() {
                            if mod_read_count.contains_key(&read.id()) {
                                let count = mod_read_count.get(&read.id()).unwrap();
                                mod_read_count.insert(read.id(), count + 1);
                            } else {
                                mod_read_count.insert(read.id(), 1);
                            }
                            mod_reads.insert(read.id(), read.clone());
                        }
                        if e_count > 0 {
                            err_count += e_count;
                        } else if err_count != 0 {
                            err_count = 0;
                        }
                        self.update_control_socks(err_count);
                    }
                }
                let mut updatable_reads: Vec<read::Read> = Vec::new();
                for (read_id, read) in mod_reads.iter() {
                    if mod_read_count.get(read_id) == Some(&upload_apis.len()) {
                        updatable_reads.push(read.clone());
                    }
                }
                if let Ok(mut sq) = self.sqlite.lock() {
                    match sq.update_reads_status(&updatable_reads) {
                        Ok(_) => {},
                        Err(e) => {
                            println!("Error updating uploaded reads: {e}");
                        }
                    }
                }
            }

            let mut upload_pause: u64 = defaults::DEFAULT_UPLOAD_INTERVAL;
            if let Ok(control) = self.control.lock() {
                upload_pause = control.upload_interval;
            }
            wait_for_next_upload(&self.waiter, Duration::from_secs(upload_pause.max(1)));
        } // end auto upload loop
        // Stopped, unless a newer run is taking over (it sets Running itself).
        let replaced = self.generation.load(Ordering::SeqCst) != my_run;
        if let Ok(mut r) = self.status.lock() {
            if !replaced || *r == Status::Stopping {
                *r = Status::Stopped;
            }
        }
        // let everyone know
        self.update_control_socks(0);
        println!("Auto upload of reads finished.")
    }

    fn update_control_socks(&self, err_count: usize) {
        // let all the control sockets know of our status
        let stat = self.status();
        for (_, sock) in crate::control::client::connected(&self.control_sockets) {
            _ = write_uploader_status(&sock, stat.clone());
        }
        if let Ok(mut inf) = self.info.lock() {
            inf.update_status(stat, err_count);
        }
    }
}

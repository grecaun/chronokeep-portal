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

use std::{sync::{Arc, Condvar, Mutex}, thread::{self, JoinHandle}, time::{Duration, Instant}};

use crate::{control, database::{Database, sqlite}, network::{api::{self, Api}, heartbeat::HeartbeatState}, reader::{self, Reader}, remote::{remote_util, uploader::info::UploadInfo}};

/// How often a heartbeat is sent to each Chronokeep Remote API with heartbeat turned on.
pub const INTERVAL: Duration = Duration::from_secs(60);
/// Per-request timeout. Kept short so stopping never waits long on a slow API.
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
/// Battery percentages above this mean the portal is charging (see battery.rs).
const CHARGING_PERCENTAGE: u8 = 110;

/// Tells the heartbeat thread to stop. Waiting on it wakes immediately when
/// stopped, so the thread never sleeps through a stop or spins while waiting.
#[derive(Default)]
pub(crate) struct StopSignal {
    stopped: Mutex<bool>,
    wake: Condvar,
}

impl StopSignal {
    pub(crate) fn stop(&self) {
        if let Ok(mut stopped) = self.stopped.lock() {
            *stopped = true;
        }
        self.wake.notify_all();
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.stopped.lock().map(|s| *s).unwrap_or(true)
    }

    /// Sleeps until `deadline` or until stopped. Returns true when stopped.
    pub(crate) fn wait_until(&self, deadline: Instant) -> bool {
        let Ok(guard) = self.stopped.lock() else { return true };
        let timeout = deadline.saturating_duration_since(Instant::now());
        match self.wake.wait_timeout_while(guard, timeout, |stopped| !*stopped) {
            Ok((guard, _)) => *guard,
            Err(_) => true,
        }
    }
}

#[derive(Clone)]
pub struct Heartbeat {
    start_time: i64,
    signal: Arc<StopSignal>,
    control: Arc<Mutex<control::Control>>,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
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
            signal: Arc::new(StopSignal::default()),
            control,
            sqlite,
            readers,
            upload_info
        }
    }

    /// Stops the heartbeat thread. It wakes right away; at most one request in
    /// progress (HTTP_TIMEOUT) finishes before `run` returns.
    pub fn stop(&self) {
        self.signal.stop();
    }

    pub fn running(&self) -> bool {
        !self.signal.is_stopped()
    }

    /// The portal's current state. `build` swaps in each API's device name.
    fn state(&self) -> HeartbeatState {
        let now = chrono::Utc::now().timestamp_millis();
        let (dev, bat, voltage, chip_type) = match self.control.lock() {
            Ok(c) => (c.name.clone(), c.battery, c.battery_voltage.clone(), c.chip_type.clone()),
            Err(_) => (String::new(), 0, String::new(), String::new()),
        };
        let (mut total, mut connected, mut reading) = (0, 0, false);
        if let Ok(readers) = self.readers.lock() {
            for read in readers.iter() {
                total += 1;
                if let Some(true) = read.is_connected() {
                    connected += 1;
                }
                if let Ok(ant) = read.antennas.lock() {
                    if ant.iter().any(|a| *a == reader::ANTENNA_STATUS_CONNECTED) {
                        reading = true;
                    }
                }
            }
        }
        let upload_errors = self.upload_info.lock().map(|u| u.total_errors).unwrap_or(0);
        HeartbeatState {
            dev: dev.clone(),
            location: dev,
            now,
            uptime: (now - self.start_time) / 1000, // Uptime in seconds
            battery: voltage,
            charging: is_charging(bat),
            hash: format!("Chronokeep Portal v{}", env!("CARGO_PKG_VERSION")),
            upload_errors,
            readers_connected: connected,
            readers_total: total,
            reading,
            chip_type,
        }
    }

    /// The heartbeat for each Remote API with heartbeat turned on.
    fn build(&self) -> Vec<(Api, HeartbeatState)> {
        let apis = match self.sqlite.lock() {
            Ok(sq) => match sq.get_apis() {
                Ok(apis) => apis,
                Err(e) => {
                    println!("Error getting all APIS: {e}");
                    return Vec::new();
                }
            },
            Err(_) => return Vec::new(),
        };
        let apis: Vec<Api> = apis.into_iter().filter(is_heartbeat_api).collect();
        if apis.is_empty() {
            return Vec::new();
        }
        with_device_names(self.state(), apis)
    }

    /// Sends a heartbeat right away, then every INTERVAL, until `stop` is called.
    pub fn run(&self) {
        let this = self.clone();
        run_loop(&self.signal, INTERVAL, move || this.build());
        println!("Heartbeat thread stopping.");
    }
}

/// Starts `Heartbeat::run` on its own thread.
pub fn spawn(heartbeat: Arc<Heartbeat>) -> JoinHandle<()> {
    thread::spawn(move || heartbeat.run())
}

/// Battery percentages above 110 mean charging (see battery.rs). Checked on the
/// raw value; capping it to 100 first would hide charging.
pub(crate) fn is_charging(battery: u8) -> bool {
    battery > CHARGING_PERCENTAGE
}

/// Only Chronokeep Remote APIs with heartbeat turned on get heartbeats.
pub(crate) fn is_heartbeat_api(api: &Api) -> bool {
    (api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE || api.kind() == api::API_TYPE_CHRONOKEEP_REMOTE_SELF) && api.heartbeat()
}

/// One heartbeat per API. Each API can have its own device name; a blank one
/// keeps the portal's name.
pub(crate) fn with_device_names(state: HeartbeatState, apis: Vec<Api>) -> Vec<(Api, HeartbeatState)> {
    apis.into_iter().map(|api| {
        let mut hb = state.clone();
        let name = api.device_name().trim();
        if !name.is_empty() {
            hb.dev = name.to_string();
            hb.location = name.to_string();
        }
        (api, hb)
    }).collect()
}

/// Sends `build()`'s heartbeats every `interval` until `signal` is stopped.
pub(crate) fn run_loop<F>(signal: &StopSignal, interval: Duration, build: F)
where
    F: Fn() -> Vec<(Api, HeartbeatState)>,
{
    let client = match remote_util::http_client(HTTP_TIMEOUT) {
        Ok(c) => c,
        Err(e) => {
            println!("Unable to create http client, heartbeat thread not started: {e}");
            return;
        }
    };
    // The first heartbeat goes out right away, then every interval.
    let mut next = Instant::now();
    while !signal.wait_until(next) {
        // Keep a steady schedule; if sending ran past the next slot, skip ahead
        // instead of sending back-to-back.
        next += interval;
        let now = Instant::now();
        if next <= now {
            next = now + interval;
        }
        for (api, hb) in build() {
            if signal.is_stopped() {
                break;
            }
            if let Err(e) = send(&client, &api, &hb) {
                println!("Error sending heartbeat to {}: {e}", api.nickname());
            }
        }
    }
}

/// Sends one heartbeat to `POST {uri}heartbeat`. The Remote API answers 200 with no body.
pub(crate) fn send(client: &reqwest::blocking::Client, api: &Api, hb: &HeartbeatState) -> Result<(), String> {
    let mut url = api.uri().trim().to_string();
    if !url.ends_with('/') {
        url.push('/');
    }
    let resp = client
        .post(format!("{url}heartbeat"))
        .bearer_auth(api.token().trim())
        .json(hb)
        .send()
        .map_err(|e| format!("error talking to api: {e}"))?;
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    // The API explains rejections (e.g. "Delete Keys Cannot Write") in {"message": ...}.
    let message = resp
        .json::<serde_json::Value>()
        .ok()
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from));
    Err(match message {
        Some(m) => format!("server returned {status}: {m}"),
        None => format!("server returned {status}"),
    })
}

#[cfg(test)]
mod tests;

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

//! Watches the portal's CPU temperature, so it can be sent with the heartbeat and warn when
//! the portal is getting too hot.

use std::{fs, sync::{Arc, Mutex}, thread, time::Duration};

use chrono::{DateTime, Local};

use crate::{control::{Control, client::ControlSockets, socket::notifications::APINotification}, database::sqlite, notifier};

/// Where Linux reports the CPU temperature, in thousandths of a degree Celsius. On a
/// Raspberry Pi this is the system on a chip, the board's only temperature sensor.
pub const TEMPERATURE_PATH: &str = "/sys/class/thermal/thermal_zone0/temp";
/// How often the temperature is checked.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(10);
/// HIGH_TEMP is sent at this temperature (°C), before a Raspberry Pi 4 starts slowing itself
/// down at 80°C.
pub const HIGH_TEMP: f32 = 75.0;
/// MAX_TEMP is sent at this temperature (°C), when a Raspberry Pi 4 starts slowing itself down.
pub const MAX_TEMP: f32 = 80.0;
/// A notification is sent again only once the temperature has dropped this far (°C) below
/// where it was sent, so hovering around a threshold doesn't send a stream of them.
pub const REARM_DROP: f32 = 5.0;

/// Reads the temperature in °C, to a tenth of a degree, from a file holding thousandths of a
/// degree. Returns None when there's no such file or it doesn't hold a number.
pub fn read_celsius(path: &str) -> Option<f32> {
    let millidegrees: i64 = fs::read_to_string(path).ok()?.trim().parse().ok()?;
    Some((millidegrees as f32 / 100.0).round() / 10.0)
}

/// Decides when to send HIGH_TEMP and MAX_TEMP: once each time the temperature reaches them,
/// and again only after it has cooled REARM_DROP below.
#[derive(Default, Debug)]
pub struct Alerts {
    high_sent: bool,
    max_sent: bool,
}

impl Alerts {
    /// Takes the latest temperature and returns the notification to send, if any.
    pub fn update(&mut self, celsius: f32) -> Option<APINotification> {
        if celsius < MAX_TEMP - REARM_DROP {
            self.max_sent = false;
        }
        if celsius < HIGH_TEMP - REARM_DROP {
            self.high_sent = false;
        }
        if celsius >= MAX_TEMP && !self.max_sent {
            // MAX_TEMP covers HIGH_TEMP too, so don't send that on the way back down
            self.max_sent = true;
            self.high_sent = true;
            return Some(APINotification::MaxTemp);
        }
        if celsius >= HIGH_TEMP && !self.high_sent {
            self.high_sent = true;
            return Some(APINotification::HighTemp);
        }
        None
    }
}

pub struct Checker {
    keepalive: Arc<Mutex<bool>>,
    control: Arc<Mutex<Control>>,
    notifier: notifier::Notifier,
    control_sockets: ControlSockets,
    sqlite: Arc<Mutex<sqlite::SQLite>>,
    alerts: Alerts,
}

impl Checker {
    pub fn new(
        keepalive: Arc<Mutex<bool>>,
        control: Arc<Mutex<Control>>,
        notifier: notifier::Notifier,
        control_sockets: ControlSockets,
        sqlite: Arc<Mutex<sqlite::SQLite>>,
    ) -> Self {
        Self { keepalive, control, notifier, control_sockets, sqlite, alerts: Alerts::default() }
    }

    /// Checks the temperature every CHECK_INTERVAL until the portal stops. Returns straight
    /// away when there's no temperature to read, leaving the temperature unknown.
    pub fn run(&mut self) {
        if read_celsius(TEMPERATURE_PATH).is_none() {
            println!("No temperature reading at {TEMPERATURE_PATH}, not watching the temperature.");
            return;
        }
        println!("Starting temperature checker thread.");
        loop {
            let celsius = read_celsius(TEMPERATURE_PATH);
            if let Ok(mut control) = self.control.lock() {
                control.temperature = celsius;
            }
            if let Some(notification) = celsius.and_then(|c| self.alerts.update(c)) {
                self.send_notification(notification);
            }
            // wait in short steps so stopping the portal isn't held up
            for _ in 0..CHECK_INTERVAL.as_secs() {
                thread::sleep(Duration::from_secs(1));
                if self.keepalive.lock().map(|k| !*k).unwrap_or(true) {
                    return;
                }
            }
        }
    }

    fn send_notification(&self, notification: APINotification) {
        let date_time: DateTime<Local> = std::time::SystemTime::now().into();
        let time = format!("{}", date_time.format("%Y/%m/%d %T"));
        match notification {
            APINotification::HighTemp => self.notifier.send_notification(notifier::Notification::HighTemp, time),
            APINotification::MaxTemp => self.notifier.send_notification(notifier::Notification::MaxTemp, time),
            _ => {},
        }
        self.notifier.send_device_notification(notification, &self.control_sockets, &self.sqlite);
    }
}

#[cfg(test)]
mod tests;

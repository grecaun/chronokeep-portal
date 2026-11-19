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

//! What /api/status reports for the temperature and battery.

use serde_json::{json, Value};

use crate::web::status_json;

fn status(temperature: Option<f32>, battery: u8, voltage: &str) -> Value {
    serde_json::from_str(&status_json(temperature, battery, voltage)).unwrap()
}

#[test]
fn unknown_without_a_sensor_or_battery_monitor() {
    // the battery voltage stays "0" when there's no battery monitor
    let s = status(None, 0, "0");
    assert_eq!(s["temperature"]["celsius"], Value::Null);
    assert_eq!(s["battery"]["percent"], Value::Null);
    assert_eq!(s["battery"]["millivolts"], Value::Null);
    assert_eq!(s["battery"]["charging"], false);
}

#[test]
fn reports_readings_with_their_warning_levels() {
    let s = status(Some(54.5), 72, "13150");
    assert_eq!(s["temperature"], json!({ "celsius": 54.5, "high": 75.0, "max": 80.0 }));
    assert_eq!(s["battery"], json!({ "percent": 72, "millivolts": 13150, "charging": false, "low": 30, "critical": 15 }));
}

#[test]
fn charging_reads_as_full() {
    // the battery monitor records anything above 110% as charging
    let s = status(None, 150, "13900");
    assert_eq!(s["battery"]["percent"], 100);
    assert_eq!(s["battery"]["charging"], true);
}

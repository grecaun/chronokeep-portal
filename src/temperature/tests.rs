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

use super::*;

/// Writes `contents` to a temporary file standing in for the kernel's temperature file.
fn temperature_file(name: &str, contents: &str) -> String {
    let path = std::env::temp_dir().join(format!("chronokeep-portal-temp-{name}"));
    fs::write(&path, contents).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn reads_thousandths_of_a_degree() {
    // the kernel writes the value followed by a newline
    assert_eq!(read_celsius(&temperature_file("normal", "54530\n")), Some(54.5));
    assert_eq!(read_celsius(&temperature_file("round", "61049\n")), Some(61.0));
    assert_eq!(read_celsius(&temperature_file("whole", "80000")), Some(80.0));
    assert_eq!(read_celsius(&temperature_file("cold", "-4250\n")), Some(-4.3));
}

#[test]
fn no_temperature_without_a_reading() {
    assert_eq!(read_celsius(&temperature_file("empty", "")), None);
    assert_eq!(read_celsius(&temperature_file("text", "hot\n")), None);
    assert_eq!(read_celsius("/no/such/thermal_zone/temp"), None);
}

fn sent(alerts: &mut Alerts, celsius: f32) -> &'static str {
    match alerts.update(celsius) {
        Some(APINotification::HighTemp) => "HIGH_TEMP",
        Some(APINotification::MaxTemp) => "MAX_TEMP",
        Some(other) => panic!("unexpected notification {other:?}"),
        None => "",
    }
}

#[test]
fn sends_each_warning_once_as_it_heats_up() {
    let mut alerts = Alerts::default();
    assert_eq!(sent(&mut alerts, 60.0), "");
    assert_eq!(sent(&mut alerts, 74.9), "");
    assert_eq!(sent(&mut alerts, 75.0), "HIGH_TEMP");
    assert_eq!(sent(&mut alerts, 77.0), "");
    assert_eq!(sent(&mut alerts, 80.0), "MAX_TEMP");
    assert_eq!(sent(&mut alerts, 83.0), "");
    // cooling a little doesn't re-send either one
    assert_eq!(sent(&mut alerts, 78.0), "");
    assert_eq!(sent(&mut alerts, 81.0), "");
}

#[test]
fn jumping_straight_past_max_sends_only_max() {
    let mut alerts = Alerts::default();
    assert_eq!(sent(&mut alerts, 82.0), "MAX_TEMP");
    assert_eq!(sent(&mut alerts, 77.0), "");
}

#[test]
fn sends_again_only_after_cooling_down() {
    let mut alerts = Alerts::default();
    assert_eq!(sent(&mut alerts, 76.0), "HIGH_TEMP");
    // hovering around the threshold sends nothing more
    for celsius in [74.0, 75.5, 72.0, 76.0, 70.1, 75.0] {
        assert_eq!(sent(&mut alerts, celsius), "", "{celsius}");
    }
    // under 70 re-arms it
    assert_eq!(sent(&mut alerts, 69.9), "");
    assert_eq!(sent(&mut alerts, 75.0), "HIGH_TEMP");

    assert_eq!(sent(&mut alerts, 80.5), "MAX_TEMP");
    assert_eq!(sent(&mut alerts, 75.5), "");
    assert_eq!(sent(&mut alerts, 80.0), "");
    assert_eq!(sent(&mut alerts, 74.9), "");
    assert_eq!(sent(&mut alerts, 80.0), "MAX_TEMP");
}

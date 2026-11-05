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

//! Checking requests from the page before they reach the portal.

use serde_json::{json, Value};

use super::error_type;
use crate::web::{check_command, ALLOWED_COMMANDS};

#[test]
fn rejects_bad_requests() {
    assert_eq!(error_type(&check_command(b"not json").unwrap_err()), "INVALID_REQUEST");
    assert_eq!(error_type(&check_command(b"").unwrap_err()), "INVALID_REQUEST");
    assert_eq!(error_type(&check_command(b"[1,2]").unwrap_err()), "INVALID_REQUEST");
    assert_eq!(error_type(&check_command(b"\"time_get\"").unwrap_err()), "INVALID_REQUEST");
    assert_eq!(error_type(&check_command(b"{}").unwrap_err()), "INVALID_REQUEST");
    assert_eq!(error_type(&check_command(b"{\"command\":5}").unwrap_err()), "INVALID_REQUEST");
}

#[test]
fn only_allowed_commands_pass() {
    for c in ["connect", "disconnect", "subscribe", "keepalive_ack", "set_noficiation", "reads_add", "reads_get_all", "TIME_GET", ""] {
        let body = json!({ "command": c }).to_string();
        assert_eq!(error_type(&check_command(body.as_bytes()).unwrap_err()), "UNKNOWN_COMMAND", "command {c:?}");
    }
    for c in ALLOWED_COMMANDS {
        if *c == "settings_set" {
            continue;
        }
        let body = json!({ "command": c }).to_string();
        assert!(check_command(body.as_bytes()).is_ok(), "command {c:?}");
    }
}

fn set(name: &str, value: Value) -> Result<(String, Value), String> {
    let body = json!({ "command": "settings_set", "settings": [{ "name": name, "value": value }] }).to_string();
    check_command(body.as_bytes())
}

#[test]
fn validates_setting_values() {
    use crate::control::*;
    let good: &[(&str, &str)] = &[
        (SETTING_PORTAL_NAME, "Finish {1}"), (SETTING_READ_WINDOW, "1"), (SETTING_READ_WINDOW, "255"),
        (SETTING_BEEP_IGNORE, "0"), (SETTING_BEEP_IGNORE, "5000"), (SETTING_BEEP_IGNORE, "-3"),
        (SETTING_UPLOAD_INTERVAL, "1"), (SETTING_VOLUME, "0"), (SETTING_VOLUME, "1"), (SETTING_VOLUME, "0.3"),
        (SETTING_PLAY_SOUND, "true"), (SETTING_ENABLE_NTFY, "false"), (SETTING_CHIP_TYPE, "HEX"),
        (SETTING_VOICE, "custom"), (SETTING_SCREEN_TYPE, "PCF8574T"), (SETTING_NTFY_URL, ""), (SETTING_NTFY_PASS, "p{a}ss\""),
    ];
    for (n, v) in good {
        assert!(set(n, json!(v)).is_ok(), "{n} = {v:?} should be accepted");
    }
    let bad: &[(&str, &str)] = &[
        (SETTING_PORTAL_NAME, ""), (SETTING_PORTAL_NAME, "   "), (SETTING_READ_WINDOW, "0"), (SETTING_READ_WINDOW, "256"),
        (SETTING_READ_WINDOW, "2.5"), (SETTING_BEEP_IGNORE, "NaN"), (SETTING_BEEP_IGNORE, "inf"), (SETTING_BEEP_IGNORE, "soon"),
        (SETTING_UPLOAD_INTERVAL, "0"), (SETTING_UPLOAD_INTERVAL, "-1"), (SETTING_VOLUME, "1.01"), (SETTING_VOLUME, "-0.1"),
        (SETTING_VOLUME, "NaN"), (SETTING_PLAY_SOUND, "TRUE"), (SETTING_PLAY_SOUND, "1"), (SETTING_CHIP_TYPE, "dec"),
        (SETTING_VOICE, "Emily"), (SETTING_SCREEN_TYPE, "LCD"),
    ];
    for (n, v) in bad {
        assert_eq!(error_type(&set(n, json!(v)).unwrap_err()), "INVALID_SETTING", "{n} = {v:?} should be refused");
    }
}

#[test]
fn refuses_unknown_settings_and_odd_shapes() {
    // The auto upload switch has its own command; the web page can't flip it as a setting.
    assert!(set("SETTING_AUTO_REMOTE", json!("true")).is_err());
    assert!(set("SETTING_SOMETHING", json!("x")).is_err());
    // Values must be strings, as the control socket expects.
    assert!(set(crate::control::SETTING_VOLUME, json!(0.5)).is_err());
    assert!(check_command(br#"{"command":"settings_set"}"#).is_err());
    assert!(check_command(br#"{"command":"settings_set","settings":{}}"#).is_err());
    // Nothing to change is fine.
    assert!(check_command(br#"{"command":"settings_set","settings":[]}"#).is_ok());
}

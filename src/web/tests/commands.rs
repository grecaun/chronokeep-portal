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

//! Running a request on the control socket, against a pretend one.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{error_type, reply_type};
use crate::web::{expected_reply, run_command, ALLOWED_COMMANDS};

/// A pretend control socket: reads the request, then plays back `script`
/// (bytes to send, pause before sending). Closes the connection afterwards if
/// `close` is set. Returns what it received.
fn fake_control(script: Vec<(&'static [u8], u64)>, close: bool) -> (SocketAddr, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
        let mut got = Vec::new();
        let mut chunk = [0u8; 4096];
        let start = Instant::now();
        while !String::from_utf8_lossy(&got).contains("\"disconnect\"") && start.elapsed() < Duration::from_secs(2) {
            match s.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => got.extend_from_slice(&chunk[..n]),
                Err(_) => {}
            }
        }
        for (bytes, pause) in script {
            thread::sleep(Duration::from_millis(pause));
            if s.write_all(bytes).is_err() {
                break;
            }
        }
        if close {
            drop(s);
        } else {
            thread::sleep(Duration::from_millis(1500));
        }
        String::from_utf8_lossy(&got).to_string()
    });
    (addr, handle)
}

const DISCONNECT: &[u8] = b"{\"command\":\"disconnect\"}\n";

#[test]
fn sends_request_then_disconnect() {
    let (addr, h) = fake_control(vec![(b"{\"command\":\"time\",\"local\":\"a\",\"utc\":\"b\"}\n", 0), (DISCONNECT, 0)], true);
    let cmd = json!({ "command": "time_get" });
    assert_eq!(reply_type(&run_command(addr, "time_get", &cmd, Duration::from_secs(5))), "time");
    let got = h.join().unwrap();
    let lines: Vec<&str> = got.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(serde_json::from_str::<Value>(lines[0]).unwrap(), cmd);
    assert_eq!(lines[1], "{\"command\":\"disconnect\"}");
}

#[test]
fn passes_over_broadcasts_to_find_the_answer() {
    let (addr, _h) = fake_control(vec![
        (b"{\"command\":\"readers\",\"readers\":[]}\n", 0),
        (b"{\"command\":\"reads\",\"list\":[]}\n", 0),
        (b"{\"command\":\"settings\",\"settings\":[{\"name\":\"SETTING_VOLUME\",\"value\":\"0.5\"}]}\n", 0),
        (b"{\"command\":\"read_auto_upload\",\"status\":\"Running\"}\n", 0),
        (DISCONNECT, 0),
    ], true);
    let reply = run_command(addr, "settings_set", &json!({ "command": "settings_set", "settings": [] }), Duration::from_secs(5));
    assert_eq!(reply_type(&reply), "settings");
}

#[test]
fn an_error_wins_over_an_answer() {
    let (addr, _h) = fake_control(vec![
        (b"{\"command\":\"error\",\"error\":{\"error_type\":\"DATABASE_ERROR\",\"message\":\"x\"}}\n", 0),
        (b"{\"command\":\"api_list\",\"apis\":[]}\n", 0),
        (DISCONNECT, 0),
    ], true);
    let reply = run_command(addr, "api_save", &json!({ "command": "api_save" }), Duration::from_secs(5));
    assert_eq!(error_type(&reply), "DATABASE_ERROR");
}

#[test]
fn messages_split_across_writes() {
    let (addr, _h) = fake_control(vec![
        (b"{\"command\":\"api_l", 0),
        (b"ist\",\"apis\":[]}\n{\"comm", 100),
        (b"and\":\"disconnect\"}\n", 100),
    ], false);
    let start = Instant::now();
    let reply = run_command(addr, "api_list", &json!({ "command": "api_list" }), Duration::from_secs(5));
    assert_eq!(reply_type(&reply), "api_list");
    // It stops at the disconnect message without waiting for the connection to close.
    assert!(start.elapsed() < Duration::from_secs(1), "took {:?}", start.elapsed());
}

#[test]
fn connection_closed_without_disconnect_message() {
    // e.g. TOO_MANY_CONNECTIONS: the portal writes an error and drops the connection.
    let (addr, _h) = fake_control(vec![(b"{\"command\":\"error\",\"error\":{\"error_type\":\"TOO_MANY_CONNECTIONS\"}}\n", 0)], true);
    let reply = run_command(addr, "api_list", &json!({ "command": "api_list" }), Duration::from_secs(5));
    assert_eq!(error_type(&reply), "TOO_MANY_CONNECTIONS");
}

#[test]
fn last_message_without_newline_before_close() {
    let (addr, _h) = fake_control(vec![(b"{\"command\":\"success\",\"count\":3}", 0)], true);
    let reply = run_command(addr, "reads_delete_all", &json!({ "command": "reads_delete_all" }), Duration::from_secs(5));
    assert_eq!(reply_type(&reply), "success");
}

#[test]
fn commands_without_a_reply_say_ok() {
    // Starting a reader only reports problems; success comes later as a readers update.
    let (addr, _h) = fake_control(vec![(DISCONNECT, 0)], true);
    let reply = run_command(addr, "reader_start", &json!({ "command": "reader_start", "id": 1 }), Duration::from_secs(5));
    assert_eq!(reply_type(&reply), "ok");
}

#[test]
fn unexpected_reply_types_are_not_the_answer() {
    let (addr, _h) = fake_control(vec![(b"{\"command\":\"readers\",\"readers\":[]}\n", 0), (DISCONNECT, 0)], true);
    let reply = run_command(addr, "time_get", &json!({ "command": "time_get" }), Duration::from_secs(5));
    assert_eq!(reply_type(&reply), "ok");
}

#[test]
fn slow_portal_reports_pending() {
    let (addr, _h) = fake_control(vec![], false);
    let start = Instant::now();
    let reply = run_command(addr, "api_remote_manual_upload", &json!({ "command": "api_remote_manual_upload" }), Duration::from_millis(700));
    assert_eq!(reply_type(&reply), "pending");
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn unreachable_control_socket() {
    // Bind then drop to get a port nothing is listening on.
    let addr = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let reply = run_command(addr, "time_get", &json!({ "command": "time_get" }), Duration::from_secs(1));
    assert_eq!(error_type(&reply), "SERVER_ERROR");
}

#[test]
fn every_allowed_command_has_a_known_reply_shape() {
    // Commands that don't produce a direct answer on success must be ones where
    // "ok" is a sensible reply.
    let no_reply = ["reader_connect", "reader_start", "quit", "shutdown", "restart"];
    for c in ALLOWED_COMMANDS {
        assert!(!expected_reply(c).is_empty() || no_reply.contains(c), "{c} has no expected reply");
    }
}

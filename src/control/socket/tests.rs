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

//! Tests for the control socket, one file per part.

use super::{find_json_end, parse_request, requests::Request};

mod messages;
mod received;
mod manual_upload;

/// Splits a buffer into messages the same way `handle_stream` does.
fn split(buffer: &str) -> Vec<String> {
    let mut buf = String::from(buffer);
    let mut out = Vec::new();
    while buf.len() > 0 {
        let mut end = find_json_end(&buf);
        if end < buf.len() {
            end += 1;
        }
        out.push(buf.drain(..end).collect());
    }
    out
}

/// Parses every message in a buffer, failing the test if one doesn't parse.
fn parse_all(buffer: &str) -> Vec<Request> {
    split(buffer).iter().map(|m| {
        parse_request(m).unwrap_or_else(|e| panic!("couldn't parse {m:?}: {e}"))
    }).collect()
}

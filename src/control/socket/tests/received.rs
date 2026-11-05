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

//! Bytes read from a control connection.

use super::parse_all;
use crate::control::socket::{push_received, requests::Request};

#[test]
fn character_split_across_reads_is_kept() {
    let msg = "{\"command\":\"settings_set\",\"settings\":[{\"name\":\"SETTING_PORTAL_NAME\",\"value\":\"Café ✓ 🏁\"}]}\n";
    let bytes = msg.as_bytes();
    // Split at every possible point, including inside each multi-byte character.
    for cut in 1..bytes.len() {
        let mut pending = Vec::new();
        let mut buffer = String::new();
        push_received(&mut pending, &mut buffer, &bytes[..cut]);
        push_received(&mut pending, &mut buffer, &bytes[cut..]);
        assert_eq!(buffer, msg, "cut at {cut}");
        assert!(pending.is_empty(), "cut at {cut}");
    }
}

#[test]
fn one_byte_at_a_time() {
    let msg = "{\"command\":\"time_get\",\"x\":\"日本\"}";
    let mut pending = Vec::new();
    let mut buffer = String::new();
    for b in msg.as_bytes() {
        push_received(&mut pending, &mut buffer, &[*b]);
    }
    assert_eq!(buffer, msg);
}

#[test]
fn invalid_bytes_do_not_lose_the_rest() {
    let mut pending = Vec::new();
    let mut buffer = String::new();
    let mut data = b"{\"command\":\"time_get\"}".to_vec();
    data.push(0xff);
    data.extend_from_slice(b"{\"command\":\"api_list\"}");
    push_received(&mut pending, &mut buffer, &data);
    assert_eq!(buffer, "{\"command\":\"time_get\"}\u{FFFD}{\"command\":\"api_list\"}");
    assert!(pending.is_empty());
    // The stray character between messages doesn't stop either from parsing.
    let reqs = parse_all(&buffer);
    assert!(reqs.iter().any(|r| matches!(r, Request::TimeGet)));
    assert!(reqs.iter().any(|r| matches!(r, Request::ApiList)));
}

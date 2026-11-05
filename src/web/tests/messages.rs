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

//! Reading messages from the control socket.

use crate::web::take_messages;

#[test]
fn takes_complete_lines_only() {
    let mut buf = b"{\"command\":\"time\"}\n\n  \r\n{\"command\":\"read".to_vec();
    let msgs = take_messages(&mut buf);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].1["command"], "time");
    assert_eq!(buf, b"{\"command\":\"read".to_vec());
    buf.extend_from_slice(b"ers\",\"readers\":[]}\r\n");
    let msgs = take_messages(&mut buf);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].1["command"], "readers");
    assert!(buf.is_empty());
}

#[test]
fn skips_garbage_and_bad_utf8() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"not json\n");
    buf.extend_from_slice(&[0xff, 0xfe, b'\n']);
    buf.extend_from_slice("{\"command\":\"settings\",\"settings\":[{\"name\":\"SETTING_PORTAL_NAME\",\"value\":\"Café\"}]}\n".as_bytes());
    let msgs = take_messages(&mut buf);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].1["settings"][0]["value"], "Café");
}

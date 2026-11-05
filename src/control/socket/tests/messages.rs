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

//! Splitting the received text into requests.

use super::{parse_all, split};
use crate::control::socket::{find_json_end, requests::Request};

#[test]
fn splits_messages_with_newlines() {
    let reqs = parse_all("{\"command\":\"time_get\"}\n{\"command\":\"api_list\"}\n");
    assert_eq!(reqs.len(), 2);
    assert!(matches!(reqs[0], Request::TimeGet));
    assert!(matches!(reqs[1], Request::ApiList));
}

#[test]
fn splits_messages_without_newlines() {
    let reqs = parse_all("{\"command\":\"time_get\"}{\"command\":\"disconnect\"}");
    assert_eq!(reqs.len(), 2);
    assert!(matches!(reqs[1], Request::Disconnect));
}

#[test]
fn keeps_nested_objects_together() {
    let reqs = parse_all(concat!(
        "{\"command\":\"settings_set\",\"settings\":[{\"name\":\"SETTING_VOLUME\",\"value\":\"0.5\"},",
        "{\"name\":\"SETTING_PLAY_SOUND\",\"value\":\"true\"}]}\n{\"command\":\"disconnect\"}\n"
    ));
    assert_eq!(reqs.len(), 2);
    match &reqs[0] {
        Request::SettingsSet { settings } => assert_eq!(settings.len(), 2),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn braces_inside_strings_do_not_split_a_message() {
    // API keys, passwords and names can contain braces.
    let reqs = parse_all(concat!(
        "{\"command\":\"api_save\",\"id\":1,\"name\":\"Remote {main}\",\"kind\":\"CHRONOKEEP_REMOTE\",",
        "\"uri\":\"\",\"token\":\"ab}c{d}}\"}\n{\"command\":\"disconnect\"}\n"
    ));
    assert_eq!(reqs.len(), 2);
    match &reqs[0] {
        Request::ApiSave { name, token, .. } => {
            assert_eq!(name, "Remote {main}");
            assert_eq!(token, "ab}c{d}}");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(matches!(reqs[1], Request::Disconnect));
}

#[test]
fn escaped_quotes_inside_strings() {
    let reqs = parse_all(concat!(
        "{\"command\":\"settings_set\",\"settings\":[{\"name\":\"SETTING_PORTAL_NAME\",\"value\":\"say \\\"}\\\" \\\\\"}]}",
        "{\"command\":\"time_get\"}"
    ));
    assert_eq!(reqs.len(), 2);
    match &reqs[0] {
        Request::SettingsSet { settings } => assert_eq!(settings[0].value(), "say \"}\" \\"),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn junk_before_a_message_does_not_lose_it() {
    for junk in ["\u{FFFD}", "garbage", "}", "]]", "\"x\""] {
        let reqs = parse_all(&format!("{{\"command\":\"time_get\"}}{junk}{{\"command\":\"api_list\"}}\n"));
        assert_eq!(reqs.len(), 2, "junk {junk:?}");
        assert!(matches!(reqs[1], Request::ApiList), "junk {junk:?}");
    }
}

#[test]
fn incomplete_message_is_left_whole() {
    // The caller puts an incomplete message back and waits for the rest.
    let buf = "{\"command\":\"settings_set\",\"settings\":[{\"name\":\"SETTING_V";
    assert_eq!(find_json_end(&String::from(buf)), buf.len() - 1);
}

#[test]
fn single_character_buffers() {
    assert_eq!(find_json_end(&String::from("{")), 0);
    assert_eq!(find_json_end(&String::from("\n")), 0);
    assert_eq!(find_json_end(&String::from("}")), 0);
}

#[test]
fn leading_whitespace_before_a_message() {
    let reqs: Vec<String> = split("\n  {\"command\":\"time_get\"}\n");
    assert_eq!(reqs.len(), 1);
    assert!(matches!(serde_json::from_str::<Request>(reqs[0].trim()).unwrap(), Request::TimeGet));
}

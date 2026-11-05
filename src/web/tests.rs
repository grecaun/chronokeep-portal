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

//! Tests for the web page's server, one file per part.

use serde_json::Value;

mod requests;
mod messages;
mod commands;

fn reply_type(reply: &str) -> String {
    let v: Value = serde_json::from_str(reply).unwrap();
    v["command"].as_str().unwrap().to_string()
}

fn error_type(reply: &str) -> String {
    let v: Value = serde_json::from_str(reply).unwrap();
    v["error"]["error_type"].as_str().unwrap_or("").to_string()
}

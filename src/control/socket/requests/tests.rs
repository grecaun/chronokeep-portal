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

//! Requests from control programs.

use super::Request;

#[test]
fn manual_upload_request_fields_are_optional() {
    // Chronokeep desktop sends no fields.
    match serde_json::from_str::<Request>("{\"command\":\"api_remote_manual_upload\"}").unwrap() {
        Request::ApiRemoteManualUpload { start_seconds, end_seconds, reader_name } => {
            assert!(start_seconds.is_none() && end_seconds.is_none() && reader_name.is_none());
        }
        other => panic!("unexpected {other:?}"),
    }
    match serde_json::from_str::<Request>(
        "{\"command\":\"api_remote_manual_upload\",\"start_seconds\":10,\"end_seconds\":20,\"reader_name\":\"Finish\"}"
    ).unwrap() {
        Request::ApiRemoteManualUpload { start_seconds, end_seconds, reader_name } => {
            assert_eq!((start_seconds, end_seconds, reader_name.as_deref()), (Some(10), Some(20), Some("Finish")));
        }
        other => panic!("unexpected {other:?}"),
    }
}

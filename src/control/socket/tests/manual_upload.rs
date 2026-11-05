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

//! Choosing which stored reads a manual upload sends.

use std::fs;

use crate::control::socket::reads_for_manual_upload;
use crate::database::{sqlite::{self, tests::setup_tests}, Database};
use crate::objects::read;

fn seeded(path: &str) -> sqlite::SQLite {
    let mut sq = setup_tests(path);
    let rows = [("1", 100, "Finish"), ("2", 200, "finish"), ("3", 300, "Start"), ("4", 400, "Finish"), ("5", -50, "Finish")];
    let reads: Vec<read::Read> = rows.iter().map(|(chip, secs, reader)| read::Read::new(
        0, String::from(*chip), *secs, 0, *secs, 0, 1, String::from(*reader), String::from("-50"), read::READ_UPLOADED_FALSE,
    )).collect();
    sq.save_reads(&reads).unwrap();
    sq
}

fn chips(reads: Vec<read::Read>) -> Vec<String> {
    let mut c: Vec<String> = reads.iter().map(|r| String::from(r.chip())).collect();
    c.sort();
    c
}

#[test]
fn manual_upload_read_selection() {
    let path = "./test_manual_upload_select.sqlite";
    let sq = seeded(path);
    let pick = |s: Option<i64>, e: Option<i64>, r: Option<&str>| chips(reads_for_manual_upload(&sq, s, e, r).unwrap());
    // Everything.
    assert_eq!(pick(None, None, None), ["1", "2", "3", "4", "5"]);
    // Inclusive at both ends.
    assert_eq!(pick(Some(200), Some(300), None), ["2", "3"]);
    // Only a start, or only an end (including a read before 1970).
    assert_eq!(pick(Some(300), None, None), ["3", "4"]);
    assert_eq!(pick(None, Some(100), None), ["1", "5"]);
    // Backwards range matches nothing.
    assert!(pick(Some(300), Some(200), None).is_empty());
    // Reader names ignore case and surrounding spaces; blank means every reader.
    assert_eq!(pick(None, None, Some("FINISH")), ["1", "2", "4", "5"]);
    assert_eq!(pick(None, None, Some(" Start ")), ["3"]);
    assert_eq!(pick(None, None, Some("")), ["1", "2", "3", "4", "5"]);
    assert_eq!(pick(None, None, Some("   ")), ["1", "2", "3", "4", "5"]);
    assert!(pick(None, None, Some("Nowhere")).is_empty());
    // Both together.
    assert_eq!(pick(Some(150), Some(450), Some("finish")), ["2", "4"]);
    // Extreme bounds.
    assert_eq!(pick(Some(i64::MIN), Some(i64::MAX), None), ["1", "2", "3", "4", "5"]);
    drop(sq);
    _ = fs::remove_file(path);
}

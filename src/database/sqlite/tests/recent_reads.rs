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

//! Most recently saved reads (shown on the web page).

use super::*;

fn seeded(path: &str) -> SQLite {
    let mut sq = setup_tests(path);
    let rows = [("1", 100, "Finish"), ("2", 200, "finish"), ("3", 300, "Start"), ("4", 400, "Finish"), ("5", -50, "Finish")];
    let reads: Vec<read::Read> = rows.iter().map(|(chip, secs, reader)| read::Read::new(
        0, String::from(*chip), *secs, 0, *secs, 0, 1, String::from(*reader), String::from("-50"), read::READ_UPLOADED_FALSE,
    )).collect();
    sq.save_reads(&reads).unwrap();
    sq
}

#[test]
fn recent_reads_newest_saved_first() {
    let path = "./test_recent_reads.sqlite";
    let sq = seeded(path);
    let recent = sq.get_recent_reads(3).unwrap();
    // Most recently saved first, whatever their read times.
    assert_eq!(recent.iter().map(|r| r.chip()).collect::<Vec<_>>(), ["5", "4", "3"]);
    assert!(recent.iter().all(|r| r.id() > 0));
    assert_eq!(sq.get_recent_reads(100).unwrap().len(), 5);
    assert!(sq.get_recent_reads(0).unwrap().is_empty());
    drop(sq);
    finalize_tests(path);
}

#[test]
fn recent_reads_show_upload_status() {
    let path = "./test_recent_reads_status.sqlite";
    let mut sq = seeded(path);
    let mut first = sq.get_recent_reads(1).unwrap();
    first[0].set_uploaded(read::READ_UPLOADED_TRUE);
    sq.update_reads_status(&first).unwrap();
    let recent = sq.get_recent_reads(2).unwrap();
    assert_eq!(recent[0].uploaded(), read::READ_UPLOADED_TRUE);
    assert_eq!(recent[1].uploaded(), read::READ_UPLOADED_FALSE);
    drop(sq);
    finalize_tests(path);
}

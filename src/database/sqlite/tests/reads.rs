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

//! Chip reads.

use super::*;

fn make_reads() -> Vec<read::Read> {
    let mut output: Vec<read::Read> = Vec::new();
    output.push(read::Read::new(
            0,
            String::from("1005"),
            1005,
            100,
            1015,
            105,
            2,
            String::from("reader-1"),
            String::from("-25dba"),
            read::READ_UPLOADED_TRUE
    ));
    output.push(read::Read::new(
            0,
            String::from("1005"),
            11005,
            90,
            11010,
            95,
            4,
            String::from("reader-1"),
            String::from("-20dba"),
            read::READ_UPLOADED_FALSE
    ));
    // this entry should be ignored on save
    output.push(read::Read::new(
            0,
            String::from("1005"),
            1005,
            100,
            1015,
            105,
            3,
            String::from("reader-1"),
            String::from("-5dba"),
            read::READ_UPLOADED_TRUE
    ));
    for i in 1006..1100 {
        output.push(read::Read::new(
            0,
            format!("{i}"),
            i,
            100,
            i+5,
            105,
            1,
            String::from("reader-1"),
            String::from("-25dba"),
            read::READ_UPLOADED_FALSE
        ));
    }
    output
}

#[test]
fn test_save_reads() {
    let unique_path = "./test_save_reads.sqlite";
    let new_reads = make_reads();
    let mut sqlite = setup_tests(unique_path);
    let result = sqlite.save_reads(&new_reads);
    assert!(result.is_ok());
    assert_eq!(new_reads.len() - 1, result.unwrap());
    // test if we can add a read we already know about, this should return 0
    let temp_read = new_reads.first().unwrap();
    let updated_read = read::Read::new(
        0,
        String::from(temp_read.chip()),
        temp_read.seconds(),
        temp_read.milliseconds(),
        temp_read.reader_seconds(),
        temp_read.reader_milliseconds(),
        500,
        String::from("new-reader-name"),
        String::from("15dba"),
        read::READ_UPLOADED_FALSE
    );
    let result = sqlite.save_reads(&vec![updated_read]);
    assert!(result.is_ok());
    assert_eq!(0, result.unwrap());
    // test if we can add a status that we don't know about -- there is no status field anymore
    let updated_read = read::Read::new(
        0,
        String::from(temp_read.chip()),
        temp_read.seconds(),
        temp_read.milliseconds(),
        temp_read.reader_seconds(),
        temp_read.reader_milliseconds(),
        500,
        String::from("new-reader-name"),
        String::from("15dba"),
        read::READ_UPLOADED_FALSE
    );
    let result = sqlite.save_reads(&vec![updated_read]);
    assert!(result.is_ok());
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_get_reads() {
    let unique_path = "./test_get_reads.sqlite";
    let new_reads = make_reads();
    let mut sqlite = setup_tests(unique_path);
    _ = sqlite.save_reads(&new_reads);
    let result = sqlite.get_reads(0, 2000);
    assert!(result.is_ok());
    let reads = result.unwrap();
    assert_eq!(new_reads.len()-2, reads.len());
    let mut count = 0;
    for outer in reads.iter() {
        let mut found = false;
        for inner in new_reads.iter() {
            if outer.equals(&inner) {
                found = true;
                break;
            }
        }
        if found {
            count = count + 1;
        }
        assert!(found)
    }
    assert_eq!(reads.len(), count);
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_get_all_reads() {
    let unique_path = "./test_get_all_reads.sqlite";
    let new_reads = make_reads();
    let mut sqlite = setup_tests(unique_path);
    _ = sqlite.save_reads(&new_reads);
    let results = sqlite.get_all_reads();
    assert!(results.is_ok());
    let reads = results.unwrap();
    // should be a duplicate read in new_reads
    assert_eq!(new_reads.len() - 1, reads.len());
    let mut count = 0;
    for outer in reads.iter() {
        let mut found = false;
        for inner in new_reads.iter() {
            if outer.equals(&inner) {
                found = true;
                break;
            }
        }
        if found {
            count = count + 1;
        }
        assert!(found)
    }
    assert_eq!(reads.len(), count);
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_delete_reads() {
    let unique_path = "./test_delete_reads.sqlite";
    let new_reads = make_reads();
    let mut sqlite = setup_tests(unique_path);
    let count = sqlite.save_reads(&new_reads).unwrap();
    let result = sqlite.delete_reads(2000, 90000);
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let reads = sqlite.get_all_reads().unwrap();
    assert_eq!(count-1, reads.len());
    let result = sqlite.delete_reads(0, 2000);
    assert!(result.is_ok());
    assert_eq!(count-1, result.unwrap());
    let reads = sqlite.get_all_reads().unwrap();
    assert_eq!(0, reads.len());
    let result = sqlite.delete_reads(0, 90000);
    assert!(result.is_ok());
    assert_eq!(0, result.unwrap());
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_delete_all_reads() {
    let unique_path = "./test_delete_all_reads.sqlite";
    let new_reads = make_reads();
    let mut sqlite = setup_tests(unique_path);
    let count = sqlite.save_reads(&new_reads).unwrap();
    let result = sqlite.delete_all_reads();
    assert!(result.is_ok());
    assert_eq!(count, result.unwrap());
    let reads = sqlite.get_all_reads().unwrap();
    assert_eq!(0, reads.len());
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_reset_reads_upload() {
    let unique_path = "./test_reset_reads_upload.sqlite";
    let new_reads = make_reads();
    let mut sqlite = setup_tests(unique_path);
    let count = sqlite.save_reads(&new_reads).unwrap();
    let not_uploaded = sqlite.get_not_uploaded_reads().unwrap();
    assert_ne!(count, not_uploaded.len());
    assert_ne!(0, not_uploaded.len());
    let result = sqlite.reset_reads_upload();
    assert!(result.is_ok());
    let res_count = result.unwrap();
    assert_eq!(count, res_count);
    let not_uploaded = sqlite.get_not_uploaded_reads().unwrap();
    assert_eq!(count, not_uploaded.len());
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_get_not_uploaded_reads() {
    let unique_path = "./test_get_not_uploaded_reads.sqlite";
    let new_reads = make_reads();
    let mut sqlite = setup_tests(unique_path);
    sqlite.save_reads(&new_reads).unwrap();
    let new_reads = sqlite.get_all_reads().unwrap();
    let mut not_uploaded = 0;
    for read in new_reads.iter() {
        if read.uploaded() == read::READ_UPLOADED_FALSE {
            not_uploaded = not_uploaded + 1;
        }
    }
    let result = sqlite.get_not_uploaded_reads();
    assert!(result.is_ok());
    let result = result.unwrap();
    assert_eq!(not_uploaded, result.len());
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_update_reads_status() {
    let unique_path = "./test_update_reads_status.sqlite";
    let new_reads = make_reads();
    let mut sqlite = setup_tests(unique_path);
    sqlite.save_reads(&new_reads).unwrap();
    let new_reads = sqlite.get_all_reads().unwrap();
    let mut updated_reads: Vec<read::Read> = Vec::new();
    for read in new_reads.iter() {
        updated_reads.push(read::Read::new(
            read.id(),
            String::from(read.chip()),
            read.seconds(),
            read.milliseconds(),
            read.reader_seconds(),
            read.reader_milliseconds(),
            read.antenna(),
            String::from(read.reader()),
            String::from(read.rssi()),
            read::READ_UPLOADED_FALSE
        ))
    }
    let updated = sqlite.update_reads_status(&updated_reads);
    assert!(updated.is_ok());
    let updated = updated.unwrap();
    assert_eq!(updated_reads.len(), updated);
    let updated = sqlite.get_all_reads().unwrap();
    for outer in updated_reads.iter() {
        let mut found = false;
        for inner in updated.iter() {
            if inner.equals(&outer) {
                found = true;
                break;
            }
        }
        assert!(found)
    }
    let mut updated_reads: Vec<read::Read> = Vec::new();
    for read in new_reads.iter() {
        updated_reads.push(read::Read::new(
            read.id(),
            String::from(read.chip()),
            read.seconds(),
            read.milliseconds(),
            read.reader_seconds(),
            read.reader_milliseconds(),
            read.antenna(),
            String::from(read.reader()),
            String::from(read.rssi()),
            read::READ_UPLOADED_TRUE
        ))
    }
    let updated = sqlite.update_reads_status(&updated_reads);
    assert!(updated.is_ok());
    let updated = updated.unwrap();
    assert_eq!(updated_reads.len(), updated);
    let updated = sqlite.get_all_reads().unwrap();
    for outer in updated_reads.iter() {
        let mut found = false;
        for inner in updated.iter() {
            if inner.equals(&outer) {
                found = true;
                break;
            }
        }
        assert!(found)
    }
    drop(sqlite);
    finalize_tests(unique_path);
}

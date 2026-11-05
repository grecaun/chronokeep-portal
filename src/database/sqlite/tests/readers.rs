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

//! Saved readers.

use super::*;

#[test]
fn test_save_reader() {
    let unique_path = "./test_save_reader.sqlite";
    let original = reader::Reader::new_no_repeaters(
        0,
        String::from(reader::READER_KIND_ZEBRA),
        String::from("zebra-1"),
        String::from("192.168.1.100"),
        zebra::DEFAULT_ZEBRA_PORT,
        reader::AUTO_CONNECT_TRUE
    );
    assert!(original.is_ok());
    let original = original.unwrap();
    let mut sqlite = setup_tests(unique_path);
    let result = sqlite.save_reader(&original);
    assert!(result.is_ok());
    // returns the row id, brand new sqlite instance, so 1 should be the id
    assert_eq!(1, result.unwrap());
    let readers = sqlite.get_readers().unwrap();
    assert_eq!(1, readers.len());
    let first = readers.first().unwrap();
    assert_eq!(original.nickname(), first.nickname());
    assert_eq!(original.kind(), first.kind());
    assert_eq!(original.ip_address(), first.ip_address());
    assert_eq!(original.port(), first.port());
    assert_eq!(original.auto_connect(), first.auto_connect());
    // Test auto update feature of the reader based on reader name
    let updated_ip = "random_ip";
    let updated_port = 12345;
    let tmp = reader::Reader::new_no_repeaters(
        0,
        String::from(original.kind()),
        String::from(original.nickname()),
        String::from(updated_ip),
        updated_port,
        reader::AUTO_CONNECT_FALSE
    );
    assert!(tmp.is_ok());
    let tmp = tmp.unwrap();
    let result = sqlite.save_reader(&tmp);
    assert!(result.is_ok());
    // second entry, row id should be 2
    let row_id = result.unwrap();
    assert_eq!(2, row_id);
    let readers = sqlite.get_readers().unwrap();
    assert_eq!(1, readers.len());
    let first = readers.first().unwrap();
    assert_eq!(row_id, first.id());
    assert_eq!(original.nickname(), first.nickname());
    assert_eq!(original.kind(), first.kind());
    assert_eq!(updated_ip, first.ip_address());
    assert_eq!(updated_port, first.port());
    assert_eq!(reader::AUTO_CONNECT_FALSE, first.auto_connect());

    // Test update feature of save_reader when id matches.
    let updated_name = "new name";
    let updated_ip = "random_ip";
    let updated_port = 12345;
    let tmp = reader::Reader::new_no_repeaters(
        2,
        String::from(original.kind()),
        String::from(updated_name),
        String::from(original.ip_address()),
        original.port(),
        reader::AUTO_CONNECT_TRUE
    );
    assert!(tmp.is_ok());
    let tmp = tmp.unwrap();
    let result = sqlite.save_reader(&tmp);
    assert!(result.is_ok());
    // the id should match the result
    let row_id = result.unwrap();
    assert_eq!(tmp.id(), row_id);
    let readers = sqlite.get_readers().unwrap();
    assert_eq!(1, readers.len());
    let first = readers.first().unwrap();
    assert_eq!(row_id, first.id());
    assert_eq!(updated_name, first.nickname());
    assert_eq!(original.kind(), first.kind());
    assert_eq!(original.ip_address(), first.ip_address());
    assert_eq!(original.port(), first.port());
    assert_eq!(reader::AUTO_CONNECT_TRUE, first.auto_connect());

    // Test invalid reader kind
    let result = sqlite.save_reader(&reader::Reader::new_internal(
        0,
        String::from(original.nickname()),
        String::from("random_type"),
        String::from(updated_ip),
        updated_port,
        0
    ));
    assert!(result.is_err());
    match result {
        Ok(_) => panic!(""),
        Err(DBError::DataInsertionError(_)) => println!("Data check verified."),
        Err(e) => {
            println!("Some other error occurred: {e}");
            panic!();
        }
    }
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_get_reader() {
    let unique_path = "./test_get_reader.sqlite";
    let original = reader::Reader::new_no_repeaters(
        0,
        String::from(reader::READER_KIND_ZEBRA),
        String::from("zebra-1"),
        String::from("192.168.1.101"),
        zebra::DEFAULT_ZEBRA_PORT + 1,
        reader::AUTO_CONNECT_FALSE
    );
    assert!(original.is_ok());
    let original = original.unwrap();
    let mut sqlite = setup_tests(unique_path);
    _ = sqlite.save_reader(&original);
    let readers = sqlite.get_readers().unwrap();
    let first = readers.first().unwrap();
    let result = sqlite.get_reader(&first.id());
    assert!(result.is_ok());
    let reader = result.unwrap();
    assert!(reader.equal(&original));
    let result = sqlite.get_reader(&-1);
    assert!(result.is_err());
    match result {
        Err(DBError::NotFound) => (),
        Err(_) => {
            panic!("Expected NotFound error but found a different error.")
        },
        Ok(_) => {
            panic!("Expected error, found something.")
        }
    }
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_get_readers() {
    let unique_path = "./test_get_readers.sqlite";
    let original = reader::Reader::new_no_repeaters(
        0,
        String::from(reader::READER_KIND_ZEBRA),
        String::from("zebra-1"),
        String::from("192.168.1.101"),
        zebra::DEFAULT_ZEBRA_PORT + 1,
        reader::AUTO_CONNECT_FALSE
    );
    assert!(original.is_ok());
    let original = original.unwrap();
    let mut sqlite = setup_tests(unique_path);
    _ = sqlite.save_reader(&original);
    let results = sqlite.get_readers();
    assert!(results.is_ok());
    let readers = results.unwrap();
    assert_eq!(1, readers.len());
    let first = readers.first().unwrap();
    assert!(first.equal(&original));
    // add a bunch of readers to test that we can get them all
    for i in 2..8 {
        let tmp = reader::Reader::new_no_repeaters(
            0,
            String::from(reader::READER_KIND_ZEBRA),
            format!("zebra-{i}"),
            format!("192.168.1.10{i}"),
            zebra::DEFAULT_ZEBRA_PORT + i,
            reader::AUTO_CONNECT_FALSE
        );
        assert!(tmp.is_ok());
        let tmp = tmp.unwrap();
        _ = sqlite.save_reader(&tmp);
    }
    let results = sqlite.get_readers();
    assert!(results.is_ok());
    let readers = results.unwrap();
    assert_eq!(7, readers.len());
    for reader in readers {
        let num = reader.port() - zebra::DEFAULT_ZEBRA_PORT;
        assert_eq!(format!("zebra-{num}"), reader.nickname());
        assert_eq!(reader::READER_KIND_ZEBRA, reader.kind());
        assert_eq!(format!("192.168.1.10{num}"), reader.ip_address());
    }
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_delete_reader() {
    let unique_path = "./test_delete_reader.sqlite";
    let original = reader::Reader::new_no_repeaters(
        0,
        String::from(reader::READER_KIND_ZEBRA),
        String::from("zebra-1"),
        String::from("192.168.1.101"),
        zebra::DEFAULT_ZEBRA_PORT + 1,
        reader::AUTO_CONNECT_FALSE
    );
    assert!(original.is_ok());
    let mut original = original.unwrap();
    let mut sqlite = setup_tests(unique_path);
    original.set_id(sqlite.save_reader(&original).unwrap());
    let readers = sqlite.get_readers().unwrap();
    assert_eq!(1, readers.len());
    let first = readers.first().unwrap();
    assert!(first.equal(&original));
    let result = sqlite.delete_reader(&original.id());
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let readers = sqlite.get_readers().unwrap();
    assert_eq!(0, readers.len());
    let result = sqlite.delete_reader(&original.id());
    assert!(result.is_ok());
    assert_eq!(0, result.unwrap());
    // test delete of a single element
    let middle = 4;
    let mut middle_ix: i64 = -1;
    for i in 0..(middle * 2) {
        let ix = sqlite.save_reader(&reader::Reader::new_no_repeaters(
            0,
            String::from(reader::READER_KIND_ZEBRA),
            format!("zebra-{i}"),
            format!("192.168.1.10{i}"),
            zebra::DEFAULT_ZEBRA_PORT,
            reader::AUTO_CONNECT_FALSE
        ).unwrap()).unwrap();
        if i == middle {
            middle_ix = ix;
        }
    }
    let readers = sqlite.get_readers().unwrap();
    assert_eq!(middle*2, readers.len());
    let result = sqlite.delete_reader(&middle_ix);
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let readers = sqlite.get_readers().unwrap();
    assert_eq!((middle*2)-1, readers.len());
    drop(sqlite);
    finalize_tests(unique_path);
}

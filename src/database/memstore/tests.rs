use core::panic;
use std::collections::HashMap;
use std::fs;
use crate::database::DBError;
use crate::database::Database;
use crate::database::memstore::MemStore;
use crate::database::sqlite;
use crate::network::api;
use crate::objects::read;
use crate::objects::setting;
use crate::reader::{self, zebra};

fn setup_tests(path: &str) -> MemStore {
    let mut output = MemStore {
        backend: sqlite::tests::setup_tests(path),
        settings: HashMap::new(),
        api: HashMap::new(),
        readers: HashMap::new(),
        reads: HashMap::new()
    };
    match output.setup() {
        Ok(_) => {},
        Err(e) => {
            println!("something went wrong during setup: {e}");
            panic!()
        }
    };
    output
}

fn finalize_tests(path: &str) {
    _ = fs::remove_file(path).is_ok();
}

#[test]
fn test_setup() {
    let unique_path = "./test_setup.sqlite";
    {
        let mut memstore = MemStore {
            backend: sqlite::tests::setup_tests(unique_path),
            settings: HashMap::new(),
            api: HashMap::new(),
            readers: HashMap::new(),
            reads: HashMap::new()
        };
        let res = memstore.setup();
        match res {
            Ok(_) => println!("Everything went ok!"),
            Err(e) => {
                println!("Something went wrong! {}", e.to_string());
                panic!();
            }
        }
    }
    finalize_tests(unique_path);
}

#[test]
fn test_set_setting() {
    let unique_path = "./test_set_setting.sqlite";
    let setting_name = "RANDOM_SETTING";
    let setting_value = "random_value";
    let setting_updated_value = "new_random_value";

    let mut memstore = setup_tests(unique_path);

    let num = memstore.set_setting(&setting::Setting::new(String::from(setting_name), String::from(setting_value)));
    match num {
        Ok(setting) => {
            assert_eq!(setting_name, setting.name());
            assert_eq!(setting_value, setting.value());
        },
        Err(e) => {
            println!("Something went wrong! {}", e);
            panic!()
        }
    }
    // make sure updating doesn't cause an error
    let num = memstore.set_setting(&setting::Setting::new(String::from(setting_name), String::from(setting_updated_value)));
    match num {
        Ok(setting) => {
            assert_eq!(setting_name, setting.name());
            assert_eq!(setting_updated_value, setting.value());
        },
        Err(e) => {
            println!("Something went wrong! {}", e);
            panic!()
        }
    }
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_get_setting() {
    let unique_path = "./test_get_setting.sqlite";
    let setting_name = "RANDOM_SETTING";
    let setting_value = "random_value";
    let setting_updated_value = "new_random_value";

    let mut memstore = setup_tests(unique_path);

    assert!(memstore.set_setting(&setting::Setting::new(String::from(setting_name), String::from(setting_value))).is_ok());
    let setting = memstore.get_setting(setting_name);
    match setting {
        Ok(setting) => {
            assert_eq!(setting_name, setting.name());
            assert_eq!(setting_value, setting.value());
        },
        Err(e) => {
            println!("Something went wrong! {}", e);
            panic!()
        }
    }
    // verify that the update function of set_setting works
    assert!(memstore.set_setting(&setting::Setting::new(String::from(setting_name), String::from(setting_updated_value))).is_ok());
    let setting = memstore.get_setting(setting_name);
    match setting {
        Ok(setting) => {
            assert_eq!(setting_name, setting.name());
            assert_eq!(setting_updated_value, setting.value());
        },
        Err(e) => {
            println!("Something went wrong! {}", e);
            panic!()
        }
    }
    drop(memstore);
    finalize_tests(unique_path);
}

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
    let mut memstore = setup_tests(unique_path);
    let result = memstore.save_reader(&original);
    assert!(result.is_ok());
    // returns the row id, brand new sqlite instance, so 1 should be the id
    assert_eq!(1, result.unwrap());
    let readers = memstore.get_readers().unwrap();
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
    let result = memstore.save_reader(&tmp);
    assert!(result.is_ok());
    // second entry, row id should be 2
    let row_id = result.unwrap();
    assert_eq!(2, row_id);
    let readers = memstore.get_readers().unwrap();
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
    let result = memstore.save_reader(&tmp);
    assert!(result.is_ok());
    // the id should match the result
    let row_id = result.unwrap();
    assert_eq!(tmp.id(), row_id);
    let readers = memstore.get_readers().unwrap();
    assert_eq!(1, readers.len());
    let first = readers.first().unwrap();
    assert_eq!(row_id, first.id());
    assert_eq!(updated_name, first.nickname());
    assert_eq!(original.kind(), first.kind());
    assert_eq!(original.ip_address(), first.ip_address());
    assert_eq!(original.port(), first.port());
    assert_eq!(reader::AUTO_CONNECT_TRUE, first.auto_connect());

    // Test invalid reader kind
    let result = memstore.save_reader(&reader::Reader::new_internal(
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
    drop(memstore);
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
    let mut memstore = setup_tests(unique_path);
    _ = memstore.save_reader(&original);
    let readers = memstore.get_readers().unwrap();
    let first = readers.first().unwrap();
    let result = memstore.get_reader(&first.id());
    assert!(result.is_ok());
    let reader = result.unwrap();
    assert!(reader.equal(&original));
    let result = memstore.get_reader(&-1);
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
    drop(memstore);
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
    let mut memstore = setup_tests(unique_path);
    _ = memstore.save_reader(&original);
    let results = memstore.get_readers();
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
        _ = memstore.save_reader(&tmp);
    }
    let results = memstore.get_readers();
    assert!(results.is_ok());
    let readers = results.unwrap();
    assert_eq!(7, readers.len());
    for reader in readers {
        let num = reader.port() - zebra::DEFAULT_ZEBRA_PORT;
        assert_eq!(format!("zebra-{num}"), reader.nickname());
        assert_eq!(reader::READER_KIND_ZEBRA, reader.kind());
        assert_eq!(format!("192.168.1.10{num}"), reader.ip_address());
    }
    drop(memstore);
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
    let mut memstore = setup_tests(unique_path);
    original.set_id(memstore.save_reader(&original).unwrap());
    let readers = memstore.get_readers().unwrap();
    assert_eq!(1, readers.len());
    let first = readers.first().unwrap();
    assert!(first.equal(&original));
    let result = memstore.delete_reader(&original.id());
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let readers = memstore.get_readers().unwrap();
    assert_eq!(0, readers.len());
    let result = memstore.delete_reader(&original.id());
    assert!(result.is_ok());
    assert_eq!(0, result.unwrap());
    // test delete of a single element
    let middle = 4;
    let mut middle_ix: i64 = -1;
    for i in 0..(middle * 2) {
        let ix = memstore.save_reader(&reader::Reader::new_no_repeaters(
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
    let readers = memstore.get_readers().unwrap();
    assert_eq!(middle*2, readers.len());
    let result = memstore.delete_reader(&middle_ix);
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let readers = memstore.get_readers().unwrap();
    assert_eq!((middle*2)-1, readers.len());
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_save_api() {
    let unique_path = "./test_save_api.sqlite";
    let original = api::Api::new(
        0,
        String::from("results-api"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
        String::from("random-token-value"),
        String::from("https:://example.com/"));
    let mut memstore = setup_tests(unique_path);
    let results = memstore.save_api(&original);
    assert!(results.is_ok());
    assert_ne!(0, results.unwrap());
    let apis = memstore.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert!(first.equal(&original));
    // test update functionality (nickname stays the same)
    let results = memstore.save_api(
        &api::Api::new(
            0,
            String::from(original.nickname()),
            String::from(api::API_TYPE_CHRONOKEEP_REMOTE_SELF),
            String::from("a-different-random-token"),
            String::from("https:://random.com/")
        ));
    assert!(results.is_ok());
    assert_ne!(0, results.unwrap());
    let apis = memstore.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert_eq!(original.nickname(), first.nickname());
    assert_ne!(original.kind(), first.kind());
    assert_ne!(original.token(), first.token());
    assert_ne!(original.uri(), first.uri());
    // test update functionality (token and uri stays the same)
    // save original and verify it updated back
    _ = memstore.save_api(&original);
    let apis = memstore.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert!(first.equal(&original));
    // save new entry
    let results = memstore.save_api(&api::Api::new(
        0,
        String::from("new-nickname"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE_SELF),
        String::from(original.token()),
        String::from(original.uri())
    ));
    assert!(results.is_ok());
    assert_ne!(0, results.unwrap());
    let apis = memstore.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert_ne!(original.nickname(), first.nickname());
    assert_ne!(original.kind(), first.kind());
    assert_eq!(original.token(), first.token());
    assert_eq!(original.uri(), first.uri());
    // attempt to save invalid type
    let result = memstore.save_api(&api::Api::new(
        0,
        String::from("invalid_type_name"),
        String::from("invalid-type"),
        String::from("random-token"),
        String::from("https:://invalid-type.com/")
    ));
    assert!(result.is_err());
    match result {
        Ok(_) => {
            println!("Expected an error...");
            panic!();
        },
        Err(DBError::DataInsertionError(_)) => println!("Expected error found!"),
        Err(e) => {
            println!("Unexpected error found... {}", e);
            panic!();
        }
    }
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_get_apis() {
    let unique_path = "./test_get_apis.sqlite";
    let original = api::Api::new(
        0,
        String::from("results-api"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
        String::from("random-token-value"),
        String::from("https:://example.com/"));
    let mut memstore = setup_tests(unique_path);
    _ = memstore.save_api(&original);
    let result = memstore.get_apis();
    assert!(result.is_ok());
    let apis = result.unwrap();
    let first = apis.first().unwrap();
    assert!(first.equal(&original));
    // test that we can add a whole bunch of api entries and retrieve them
    for i in 0..5 {
        _ = memstore.save_api(&api::Api::new(
            0,
            format!("api-{i}"),
            String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
            format!("token-number-10302031{i}"),
            String::from("https::api.chronokeep.com/")
        ))
    }
    let apis = memstore.get_apis().unwrap();
    assert_eq!(6, apis.len());
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_delete_api() {
    let unique_path = "./test_delete_api.sqlite";
    let original = api::Api::new(
        0,
        String::from("results-api"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
        String::from("random-token-value"),
        String::from("https:://example.com/"));
    let mut memstore = setup_tests(unique_path);
    let orig_id = memstore.save_api(&original).unwrap_or(0);
    let apis = memstore.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let result = memstore.delete_api(&orig_id);
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let apis = memstore.get_apis().unwrap();
    assert_eq!(0, apis.len());
    let result = memstore.delete_api(&orig_id);
    assert!(result.is_ok());
    assert_eq!(0, result.unwrap());
    // verify that we only delete one from a list of apis
    let mut five_id: i64 = 0;
    for i in 0..10 {
        let tmp = memstore.save_api(&api::Api::new(
            0,
            format!("results-api-{i}"),
            String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
            format!("random-token-value-{i}"),
            String::from("https:://example.com/")
        ));
        if i == 5 {
            five_id = tmp.unwrap_or(-1);
        }
    }
    let apis = memstore.get_apis().unwrap();
    assert_eq!(10, apis.len());
    let result = memstore.delete_api(&five_id);
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let apis = memstore.get_apis().unwrap();
    assert_eq!(9, apis.len());
    drop(memstore);
    finalize_tests(unique_path);
}

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
    let mut memstore = setup_tests(unique_path);
    let result = memstore.save_reads(&new_reads);
    assert!(result.is_ok());
    assert_eq!(new_reads.len() - 1, result.unwrap().len());
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
    let result = memstore.save_reads(&vec![updated_read]);
    assert!(result.is_ok());
    assert_eq!(0, result.unwrap().len());
    // test if we can add a status that we don't know about
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
    let result = memstore.save_reads(&vec![updated_read]);
    assert!(result.is_err());
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_get_reads() {
    let unique_path = "./test_get_reads.sqlite";
    let new_reads = make_reads();
    let mut memstore = setup_tests(unique_path);
    _ = memstore.save_reads(&new_reads);
    let result = memstore.get_reads(0, 2000);
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
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_get_all_reads() {
    let unique_path = "./test_get_all_reads.sqlite";
    let new_reads = make_reads();
    let mut memstore = setup_tests(unique_path);
    _ = memstore.save_reads(&new_reads);
    let results = memstore.get_all_reads();
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
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_delete_reads() {
    let unique_path = "./test_delete_reads.sqlite";
    let new_reads = make_reads();
    let mut memstore = setup_tests(unique_path);
    let saved = memstore.save_reads(&new_reads).unwrap();
    let result = memstore.delete_reads(2000, 90000);
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let reads = memstore.get_all_reads().unwrap();
    assert_eq!(saved.len()-1, reads.len());
    let result = memstore.delete_reads(0, 2000);
    assert!(result.is_ok());
    assert_eq!(saved.len()-1, result.unwrap());
    let reads = memstore.get_all_reads().unwrap();
    assert_eq!(0, reads.len());
    let result = memstore.delete_reads(0, 90000);
    assert!(result.is_ok());
    assert_eq!(0, result.unwrap());
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_delete_all_reads() {
    let unique_path = "./test_delete_all_reads.sqlite";
    let new_reads = make_reads();
    let mut memstore = setup_tests(unique_path);
    let saved = memstore.save_reads(&new_reads).unwrap();
    let result = memstore.delete_all_reads();
    assert!(result.is_ok());
    assert_eq!(saved.len(), result.unwrap());
    let reads = memstore.get_all_reads().unwrap();
    assert_eq!(0, reads.len());
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_reset_reads_upload() {
    let unique_path = "./test_reset_reads_upload.sqlite";
    let new_reads = make_reads();
    let mut memstore = setup_tests(unique_path);
    let saved = memstore.save_reads(&new_reads).unwrap();
    let not_uploaded = memstore.get_not_uploaded_reads().unwrap();
    assert_ne!(saved.len(), not_uploaded.len());
    assert_ne!(0, not_uploaded.len());
    let result = memstore.reset_reads_upload();
    assert!(result.is_ok());
    let res_count = result.unwrap();
    assert_eq!(saved.len(), res_count);
    let not_uploaded = memstore.get_not_uploaded_reads().unwrap();
    assert_eq!(saved.len(), not_uploaded.len());
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_get_not_uploaded_reads() {
    let unique_path = "./test_get_not_uploaded_reads.sqlite";
    let new_reads = make_reads();
    let mut memstore = setup_tests(unique_path);
    memstore.save_reads(&new_reads).unwrap();
    let new_reads = memstore.get_all_reads().unwrap();
    let mut not_uploaded = 0;
    for read in new_reads.iter() {
        if read.uploaded() == read::READ_UPLOADED_FALSE {
            not_uploaded = not_uploaded + 1;
        }
    }
    let result = memstore.get_not_uploaded_reads();
    assert!(result.is_ok());
    let result = result.unwrap();
    assert_eq!(not_uploaded, result.len());
    drop(memstore);
    finalize_tests(unique_path);
}

#[test]
fn test_update_reads_status() {
    let unique_path = "./test_update_reads_status.sqlite";
    let new_reads = make_reads();
    let mut memstore = setup_tests(unique_path);
    memstore.save_reads(&new_reads).unwrap();
    let new_reads = memstore.get_all_reads().unwrap();
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
    let updated = memstore.update_reads_status(&updated_reads);
    assert!(updated.is_ok());
    let updated = updated.unwrap();
    assert_eq!(updated_reads.len(), updated);
    let updated = memstore.get_all_reads().unwrap();
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
    let updated = memstore.update_reads_status(&updated_reads);
    assert!(updated.is_ok());
    let updated = updated.unwrap();
    assert_eq!(updated_reads.len(), updated);
    let updated = memstore.get_all_reads().unwrap();
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
    drop(memstore);
    finalize_tests(unique_path);
}
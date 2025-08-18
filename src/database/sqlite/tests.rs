use core::panic;
use std::fs;
use super::SQLite;
use crate::database::DBError;
use crate::database::Database;
use crate::network::api;
use crate::objects::read;
use crate::objects::setting;
use crate::reader::{self, zebra};

fn setup_tests(path: &str) -> SQLite {
    let new_conn = rusqlite::Connection::open(path).unwrap();
    let drop_tables = [
        "DROP TABLE IF EXISTS sightings;",
        "DROP TABLE IF EXISTS results_api;",
        "DROP TABLE IF EXISTS participants;",
        "DROP TABLE IF EXISTS readers;",
        "DROP TABLE IF EXISTS chip_reads;",
        "DROP TABLE IF EXISTS settings;",
    ];
    for table in drop_tables {
        if let Err(v) = new_conn.execute(table, []) {
            println!("Something went wrong while dropping a table! {table} {v}");
            panic!();
        }
    }
    let mut output = SQLite {
        conn: new_conn
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

fn setup_v1(path: &str) -> SQLite {
    let mut new_conn = rusqlite::Connection::open(path).unwrap();
    if let Ok(tx) = new_conn.transaction() {
        let database_tables = [
            "CREATE TABLE IF NOT EXISTS settings (
                setting VARCHAR NOT NULL,
                value VARCHAR NOT NULL,
                UNIQUE (setting) ON CONFLICT REPLACE
            );",
            "CREATE TABLE IF NOT EXISTS results_api (
                api_id INTEGER PRIMARY KEY AUTOINCREMENT,
                nickname VARCHAR(75),
                kind VARCHAR(50),
                token VARCHAR(100),
                uri VARCHAR(150),
                UNIQUE (nickname) ON CONFLICT REPLACE,
                UNIQUE (uri, token) ON CONFLICT REPLACE
            );",
            "CREATE TABLE IF NOT EXISTS readers (
                reader_id INTEGER PRIMARY KEY AUTOINCREMENT,
                nickname VARCHAR(75) NOT NULL,
                kind VARCHAR(50) NOT NULL,
                ip_address VARCHAR(100) NOT NULL,
                port INTEGER NOT NULL,
                auto_connect INTEGER NOT NULL DEFAULT 0,
                UNIQUE (nickname) ON CONFLICT REPLACE
            );",
            "CREATE TABLE IF NOT EXISTS chip_reads (
                chip_id INTEGER PRIMARY KEY AUTOINCREMENT,
                chip VARCHAR(100) NOT NULL,
                seconds BIGINT NOT NULL,
                milliseconds INTEGER NOT NULL,
                antenna INTEGER,
                reader VARCHAR(75),
                rssi VARCHAR(10),
                uploaded SMALLINT NOT NULL DEFAULT 0,
                UNIQUE (chip, seconds, milliseconds) ON CONFLICT IGNORE
            );",
        ];
        for table in database_tables {
            if let Err(e) = tx.execute(table, ()) {
                panic!("{}", e)
            }
        }
        if let Err(e) = tx.execute(
            "INSERT INTO settings (setting, value) VALUES (?1, ?2);",
            (super::DATABASE_VERSION_SETTING, super::DATABASE_VERSION.to_string())
        ) {
            panic!("{}", e)
        }
        if let Err(e) = tx.commit() {
            panic!("{}", e)
        }
    }
    SQLite {
        conn: new_conn
    }
}

#[test]
fn test_setup() {
    let unique_path = "./test_setup.sqlite";
    {
        let new_conn = rusqlite::Connection::open(unique_path);
        assert!(new_conn.is_ok());
        let mut sqlite = SQLite {
            conn: new_conn.unwrap()
        };
        let res = sqlite.setup();
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
fn test_update() {
    let unique_path = "./test_update.sqlite";
    {
        let mut sqlite = setup_v1(unique_path);
        match sqlite.update(1, 1) {
            Ok(_) => println!("Everything went ok!"),
            Err(e) => {
                println!("Something went wrong! {}", e);
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

    let sqlite = setup_tests(unique_path);

    let num = sqlite.set_setting(&setting::Setting::new(String::from(setting_name), String::from(setting_value)));
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
    let num = sqlite.set_setting(&setting::Setting::new(String::from(setting_name), String::from(setting_updated_value)));
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
    drop(sqlite);
    finalize_tests(unique_path);
}

#[test]
fn test_get_setting() {
    let unique_path = "./test_get_setting.sqlite";
    let setting_name = "RANDOM_SETTING";
    let setting_value = "random_value";
    let setting_updated_value = "new_random_value";

    let sqlite = setup_tests(unique_path);

    assert!(sqlite.set_setting(&setting::Setting::new(String::from(setting_name), String::from(setting_value))).is_ok());
    let setting = sqlite.get_setting(setting_name);
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
    assert!(sqlite.set_setting(&setting::Setting::new(String::from(setting_name), String::from(setting_updated_value))).is_ok());
    let setting = sqlite.get_setting(setting_name);
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
    drop(sqlite);
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
    let sqlite = setup_tests(unique_path);
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
    let sqlite = setup_tests(unique_path);
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
    let sqlite = setup_tests(unique_path);
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
    let sqlite = setup_tests(unique_path);
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

#[test]
fn test_save_api() {
    let unique_path = "./test_save_api.sqlite";
    let original = api::Api::new(
        0,
        String::from("results-api"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
        String::from("random-token-value"),
        String::from("https:://example.com/"));
    let sqlite = setup_tests(unique_path);
    let results = sqlite.save_api(&original);
    assert!(results.is_ok());
    assert_ne!(0, results.unwrap());
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert!(first.equal(&original));
    // test update functionality (nickname stays the same)
    let results = sqlite.save_api(
        &api::Api::new(
            0,
            String::from(original.nickname()),
            String::from(api::API_TYPE_CHRONOKEEP_REMOTE_SELF),
            String::from("a-different-random-token"),
            String::from("https:://random.com/")
        ));
    assert!(results.is_ok());
    assert_ne!(0, results.unwrap());
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert_eq!(original.nickname(), first.nickname());
    assert_ne!(original.kind(), first.kind());
    assert_ne!(original.token(), first.token());
    assert_ne!(original.uri(), first.uri());
    // test update functionality (token and uri stays the same)
    // save original and verify it updated back
    _ = sqlite.save_api(&original);
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert!(first.equal(&original));
    // save new entry
    let results = sqlite.save_api(&api::Api::new(
        0,
        String::from("new-nickname"),
        String::from(api::API_TYPE_CHRONOKEEP_REMOTE_SELF),
        String::from(original.token()),
        String::from(original.uri())
    ));
    assert!(results.is_ok());
    assert_ne!(0, results.unwrap());
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let first = apis.first().unwrap();
    assert_ne!(original.nickname(), first.nickname());
    assert_ne!(original.kind(), first.kind());
    assert_eq!(original.token(), first.token());
    assert_eq!(original.uri(), first.uri());
    // attempt to save invalid type
    let result = sqlite.save_api(&api::Api::new(
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
    drop(sqlite);
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
    let sqlite = setup_tests(unique_path);
    _ = sqlite.save_api(&original);
    let result = sqlite.get_apis();
    assert!(result.is_ok());
    let apis = result.unwrap();
    let first = apis.first().unwrap();
    assert!(first.equal(&original));
    // test that we can add a whole bunch of api entries and retrieve them
    for i in 0..5 {
        _ = sqlite.save_api(&api::Api::new(
            0,
            format!("api-{i}"),
            String::from(api::API_TYPE_CHRONOKEEP_REMOTE),
            format!("token-number-10302031{i}"),
            String::from("https::api.chronokeep.com/")
        ))
    }
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(6, apis.len());
    drop(sqlite);
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
    let sqlite = setup_tests(unique_path);
    let orig_id = sqlite.save_api(&original).unwrap_or(0);
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(1, apis.len());
    let result = sqlite.delete_api(&orig_id);
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(0, apis.len());
    let result = sqlite.delete_api(&orig_id);
    assert!(result.is_ok());
    assert_eq!(0, result.unwrap());
    // verify that we only delete one from a list of apis
    let mut five_id: i64 = 0;
    for i in 0..10 {
        let tmp = sqlite.save_api(&api::Api::new(
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
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(10, apis.len());
    let result = sqlite.delete_api(&five_id);
    assert!(result.is_ok());
    assert_eq!(1, result.unwrap());
    let apis = sqlite.get_apis().unwrap();
    assert_eq!(9, apis.len());
    drop(sqlite);
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
    let result = sqlite.save_reads(&vec![updated_read]);
    assert!(result.is_err());
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
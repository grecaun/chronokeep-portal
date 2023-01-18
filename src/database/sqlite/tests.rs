use core::panic;
use std::sync;
use std::fs;
use super::SQLite;
use crate::database::Database;
use crate::objects::setting;

const TEST_DATABASE_PATH: &str = "./test_db.sqlite";
const TEST_SETUP_DB_PATH: &str = "./test_setup.sqlite";

fn test_new() -> SQLite {
    let new_conn = rusqlite::Connection::open(TEST_DATABASE_PATH).unwrap();
    let output = SQLite {
        mutex: sync::Mutex::new(new_conn)
    };
    match output.setup() {
        Ok(_) => {},
        Err(_) => panic!()
    };
    output
}

fn setup_v1() -> SQLite {
    let mut new_conn = rusqlite::Connection::open(TEST_SETUP_DB_PATH).unwrap();
    if let Ok(tx) = new_conn.transaction() {
        let database_tables = [
            "CREATE TABLE IF NOT EXISTS settings (
                setting VARCHAR NOT NULL,
                value VARCHAR NOT NULL,
                UNIQUE (setting) ON CONFLICT REPLACE
            );",
            "CREATE TABLE IF NOT EXISTS results_api (
                id INTEGER PRIMARY KEY,
                nickname VARCHAR(75),
                kind VARCHAR(50),
                token VARCHAR(100),
                uri VARCHAR(150),
                UNIQUE (uri, token) ON CONFLICT REPLACE
            );",
            "CREATE TABLE IF NOT EXISTS participants (
                id INTEGER PRIMARY KEY,
                bib VARCHAR(50) NOT NULL,
                first VARCHAR(50) NOT NULL,
                last VARCHAR(75) NOT NULL,
                age INTEGER NOT NULL DEFAULT 0,
                gender VARCHAR(10) NOT NULL DEFAULT 'u',
                age_group VARCHAR(100) NOT NULL,
                distance VARCHAR(75) NOT NULL,
                part_chip VARCHAR(100) NOT NULL UNIQUE,
                anonymous SMALLINT NOT NULL DEFAULT 0,
                UNIQUE (bib, first, last, distance) ON CONFLICT REPLACE
            );",
            "CREATE TABLE IF NOT EXISTS readers (
                id INTEGER PRIMARY KEY,
                nickname VARCHAR(75) NOT NULL,
                kind VARCHAR(50) NOT NULL,
                ip_address VARCHAR(100) NOT NULL,
                port INTEGER NOT NULL,
                UNIQUE (nickname) ON CONFLICT REPLACE
            );",
            "CREATE TABLE IF NOT EXISTS chip_reads (
                id INTEGER PRIMARY KEY,
                chip VARCHAR(100) NOT NULL,
                seconds BIGINT NOT NULL,
                milliseconds INTEGER NOT NULL,
                antenna INTEGER,
                reader VARCHAR(75),
                rssi VARCHAR(10),
                status INTEGER NOT NULL DEFAULT 0,
                UNIQUE (chip, seconds, milliseconds) ON CONFLICT IGNORE
            );"
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
        mutex: sync::Mutex::new(new_conn)
    }
}

#[test]
fn test_setup() {
    {
        let new_conn = rusqlite::Connection::open(TEST_SETUP_DB_PATH);
        assert!(new_conn.is_ok());
        let sqlite = SQLite {
            mutex: sync::Mutex::new(new_conn.unwrap())
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
    assert!(fs::remove_file(TEST_SETUP_DB_PATH).is_ok())
}

#[test]
fn test_update() {
    {
        let sqlite = setup_v1();
        let mut conn = match sqlite.mutex.lock() {
            Ok(c) => c,
            Err(e) => panic!("{}", e)
        };
        match sqlite.update(&mut conn, 1, 1) {
            Ok(_) => println!("Everything went ok!"),
            Err(e) => {
                println!("Something went wrong! {}", e);
                panic!();
            }
        }
    }
    assert!(fs::remove_file(TEST_SETUP_DB_PATH).is_ok())
}

#[test]
fn test_set_setting() {
    let setting_name = "RANDOM_SETTING";
    let setting_value = "random_value";
    let setting_updated_value = "new_random_value";

    let sqlite = test_new();

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
}

#[test]
fn test_get_setting() {
    let setting_name = "RANDOM_SETTING";
    let setting_value = "random_value";
    let setting_updated_value = "new_random_value";

    let sqlite = test_new();

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
}

#[test]
fn test_save_reader() {

}

#[test]
fn test_get_readers() {

}

#[test]
fn test_delete_reader() {

}

#[test]
fn test_save_api() {

}

#[test]
fn test_get_apis() {

}

#[test]
fn test_delete_api() {

}

#[test]
fn test_save_reads() {

}

#[test]
fn test_get_reads() {

}

#[test]
fn test_delete_reads() {

}

#[test]
fn test_add_participants() {

}

#[test]
fn test_delete_participants() {

}

#[test]
fn test_delete_participant() {

}
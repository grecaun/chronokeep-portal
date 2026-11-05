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

use std::fs;
use super::SQLite;
use crate::database::DBError;
use crate::database::Database;
use crate::network::api;
use crate::objects::read;
use crate::objects::setting;
use crate::reader::{self, zebra};

pub fn setup_tests(path: &str) -> SQLite {
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

// Tests for each part of the database, sharing the helpers above.
mod setup;
mod settings;
mod readers;
mod apis;
mod reads;
mod recent_reads;

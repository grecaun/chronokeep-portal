use crate::objects::{setting, read};
use crate::network::api;
use crate::database::DBError;
use crate::reader;

use std::env;
use std::path::Path;
use std::str::FromStr;

#[cfg(test)]
mod tests;

const DATABASE_URI: &str = "./chronokeep-portal.sqlite";

const DATABASE_VERSION_SETTING: &str = "PORTAL_DATABASE_VERSION";
const DATABASE_VERSION: u16 = 4;

const DATABASE_PATH_ENV: &str = "PORTAL_DATABASE_PATH";

pub struct SQLite {
    conn: rusqlite::Connection,
}

struct TempReader {
    id: i64,
    nickname: String,
    kind: String,
    ip_address: String,
    port: u16,
    auto_connect: u8,
}

impl SQLite {
    pub fn new() -> Result<SQLite, DBError> {
        if let Ok(db_path) = env::var(DATABASE_PATH_ENV) {
            let new_conn = rusqlite::Connection::open(db_path);
            match new_conn {
                Ok(c) => 
                    Ok(SQLite {
                        conn: c,
                    }),
                Err(e) => Err(DBError::ConnectionError(e.to_string()))
            }
        } else {
            let new_conn = rusqlite::Connection::open(DATABASE_URI);
            match new_conn {
                Ok(c) => 
                    Ok(SQLite {
                        conn: c,
                    }),
                Err(e) => Err(DBError::ConnectionError(e.to_string()))
            }
        }
    }

    pub fn already_exists() -> bool {
        match Path::try_exists(Path::new(DATABASE_URI)) {
            Ok(val) => val,
            Err(_) => false,
        }
    }

    fn update(&mut self, old_version: u16, new_version: u16) -> Result<(), DBError> {
        if old_version < new_version {
            if old_version < 2 {
                if let Err(e) = self.update_to_v2() {
                    return Err(e)
                }
            }
            if old_version < 3 {
                if let Err(e) = self.update_to_v3() {
                    return Err(e)
                }
            }
            if old_version < 4 {
                if let Err(e) = self.update_to_v4() {
                    return Err(e)
                }
            }
        } else if new_version < old_version {
            return Err(DBError::DatabaseTooNew(String::from("database version is newer than our known version")))
        }
        return Ok(())
    }

    fn update_to_v4(&mut self) -> Result<(), DBError> {
        if let Ok(tx) = self.conn.transaction() {
            let updates = [
                "ALTER TABLE participants DROP COLUMN age;",
                "ALTER TABLE participants ADD COLUMN birthdate VARCHAR(50) NOT NULL DEFAULT '';",
            ];
            for table in updates {
                if let Err(e) = tx.execute(table, ()) {
                    return Err(DBError::DataInsertionError(e.to_string()))
                }
            }
            if let Err(e) = tx.execute(
                "INSERT INTO settings (setting, value) VALUES (?1, ?2);",
                (DATABASE_VERSION_SETTING, "4")
            ) {
                return Err(DBError::DataInsertionError(e.to_string()))
            }
            if let Err(e) = tx.commit() {
                return Err(DBError::DataInsertionError(e.to_string()))
            }
            return Ok(())
        }
        Err(DBError::ConnectionError(String::from("unable to start transaction")))
    }

    fn update_to_v3(&mut self) -> Result<(), DBError> {
        if let Ok(tx) = self.conn.transaction() {
            let updates = [
                "CREATE TABLE IF NOT EXISTS bibchip (
                    bib VARCHAR(50),
                    chip VARCHAR(100),
                    UNIQUE (bib, chip) ON CONFLICT REPLACE,
                    UNIQUE (chip) ON CONFLICT REPLACE
                );",
                "CREATE TABLE IF NOT EXISTS participants_new (
                    part_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    bib VARCHAR(50) NOT NULL,
                    first VARCHAR(50) NOT NULL,
                    last VARCHAR(75) NOT NULL,
                    age INTEGER NOT NULL DEFAULT 0,
                    gender VARCHAR(10) NOT NULL DEFAULT 'u',
                    age_group VARCHAR(100) NOT NULL,
                    distance VARCHAR(75) NOT NULL,
                    anonymous SMALLINT NOT NULL DEFAULT 0,
                    UNIQUE (bib) ON CONFLICT REPLACE
                );",
                "INSERT INTO bibchip SELECT bib, part_chip FROM participants;",
                "INSERT INTO participants_new SELECT part_id, bib, first, last, age, gender, age_group, distance, anonymous FROM participants;",
                "DROP TABLE participants;",
                "ALTER TABLE participants_new RENAME TO participants;"
            ];
            for table in updates {
                if let Err(e) = tx.execute(table, ()) {
                    return Err(DBError::DataInsertionError(e.to_string()))
                }
            }
            if let Err(e) = tx.execute(
                "INSERT INTO settings (setting, value) VALUES (?1, ?2);",
                (DATABASE_VERSION_SETTING, "3")
            ) {
                return Err(DBError::DataInsertionError(e.to_string()))
            }
            if let Err(e) = tx.commit() {
                return Err(DBError::DataInsertionError(e.to_string()))
            }
            return Ok(())
        }
        Err(DBError::ConnectionError(String::from("unable to start transaction")))
    }

    fn update_to_v2(&mut self) -> Result<(), DBError> {
        if let Ok(tx) = self.conn.transaction() {
            let updates = [
                "ALTER TABLE chip_reads ADD COLUMN reader_seconds BIGINT NOT NULL DEFAULT 0;",
                "ALTER TABLE chip_reads ADD COLUMN reader_milliseconds INTEGER NOT NULL DEFAULT 0;"
            ];
            for table in updates {
                if let Err(e) = tx.execute(table, ()) {
                    return Err(DBError::DataInsertionError(e.to_string()))
                }
            }
            if let Err(e) = tx.execute(
                "INSERT INTO settings (setting, value) VALUES (?1, ?2);",
                (DATABASE_VERSION_SETTING, "2")
            ) {
                return Err(DBError::DataInsertionError(e.to_string()))
            }
            if let Err(e) = tx.commit() {
                return Err(DBError::DataInsertionError(e.to_string()))
            }
            return Ok(())
        }
        Err(DBError::ConnectionError(String::from("unable to start transaction")))
    }

    fn make_tables(&mut self) -> Result<(), DBError> {
        if let Ok(tx) = self.conn.transaction() {
            let database_tables = [
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
                    reader_seconds BIGINT NOT NULL,
                    reader_milliseconds INTEGER NOT NULL,
                    antenna INTEGER,
                    reader VARCHAR(75),
                    rssi VARCHAR(10),
                    uploaded SMALLINT NOT NULL DEFAULT 0,
                    UNIQUE (chip, seconds, milliseconds) ON CONFLICT IGNORE
                );",
            ];
            for table in database_tables {
                if let Err(e) = tx.execute(table, ()) {
                    return Err(DBError::DataInsertionError(e.to_string()))
                }
            }
            if let Err(e) = tx.execute(
                "INSERT INTO settings (setting, value) VALUES (?1, ?2);",
                (DATABASE_VERSION_SETTING, DATABASE_VERSION.to_string())
            ) {
                return Err(DBError::DataInsertionError(e.to_string()))
            }
            if let Err(e) = tx.commit() {
                return Err(DBError::DataInsertionError(e.to_string()))
            }
            return Ok(())
        }
        Err(DBError::ConnectionError(String::from("unable to start transaction")))
    }
}

impl super::Database for SQLite {
    // Setup
    fn setup(&mut self) -> Result<(), DBError> {
        // If our settings table doesn't exist we run into an error we
        // can't check for when we try to retrieve the database version value.
        match self.conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                setting VARCHAR NOT NULL,
                value VARCHAR NOT NULL,
                UNIQUE (setting) ON CONFLICT REPLACE
            );",
            []
        ) {
            Ok(_) => {},
            Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
        }
        // Get the results of the version check.
        // This could cause issues if the UNIQUE trait on settings.setting fails.
        match self.conn.query_row("SELECT setting, value FROM settings WHERE setting=?1;",
            [DATABASE_VERSION_SETTING],
            |row| {
                Ok(setting::Setting::new(row.get(0)?, row.get(1)?))
        }) {
            Ok(it) => {
                if let Ok(v) = u16::from_str(&it.value()) {
                    return self.update(v, DATABASE_VERSION)
                }
                return Err(DBError::DataRetrievalError(String::from("error parsing version value")))
            },
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return self.make_tables()
            },
            Err(err) => return Err(DBError::DataRetrievalError(format!("{}",err)))
        };
    }

    // Settings
    fn set_setting(&self, setting: &setting::Setting) -> Result<setting::Setting, DBError> {
        // Block until we can do something.
        let res = self.conn.execute(
            "INSERT INTO settings (setting, value) VALUES (?1, ?2);",
            (setting.name(), setting.value()),
        );
        match res {
            Ok(_) => return Ok(setting::Setting::new(
                String::from(setting.name()),
                String::from(setting.value()))),
            Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
        }
    }

    fn get_setting(&self, name: &str) -> Result<setting::Setting, DBError> {
        match self.conn.query_row("SELECT setting, value FROM settings WHERE setting=?1;",
            [name],
            |row| {
                Ok(setting::Setting::new(row.get(0)?, row.get(1)?))
        }) {
            Ok(it) => return Ok(it),
            Err(rusqlite::Error::QueryReturnedNoRows) => return Err(DBError::NotFound),
            Err(err) => return Err(DBError::DataRetrievalError(err.to_string())),
        };
    }

    // Readers
    fn save_reader(&self, reader: &reader::Reader) -> Result<i64, DBError> {
        match reader.kind() {
            reader::READER_KIND_ZEBRA => {},
            reader::READER_KIND_IMPINJ => return Err(DBError::DataInsertionError(String::from("not yet implemented"))),
            reader::READER_KIND_RFID => return Err(DBError::DataInsertionError(String::from("not yet implemented"))),
            _ => return Err(DBError::DataInsertionError(String::from("unknown reader kind specified")))
        }
        // if our id is set to a number greater than 0 we should be updating
        if reader.id() > 0 {
            match self.conn.execute(
                "UPDATE readers SET nickname=?1, kind=?2, ip_address=?3, port=?4, auto_connect=?5 WHERE reader_id=?6;",
                (reader.nickname(), reader.kind(), reader.ip_address(), reader.port(), reader.auto_connect(), reader.id()),
            ) {
                Ok(_) => return Ok(reader.id()),
                Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
            }
        // otherwise add a new reader
        } else {
            match self.conn.execute(
                "INSERT INTO readers (nickname, kind, ip_address, port, auto_connect) VALUES (?1, ?2, ?3, ?4, ?5);",
                (reader.nickname(), reader.kind(), reader.ip_address(), reader.port(), reader.auto_connect()),
            ) {
                Ok(_) => return Ok(self.conn.last_insert_rowid()),
                Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
            }
        }
    }

    fn get_reader(&self, id: &i64) -> Result<reader::Reader, DBError> {
        match self.conn.query_row("SELECT reader_id, nickname, kind, ip_address, port, auto_connect FROM readers WHERE reader_id=?1;",
            [id],
            |row| {
                Ok(TempReader {
                    id: row.get(0)?,
                    nickname: row.get(1)?,
                    kind: row.get(2)?,
                    ip_address: row.get(3)?,
                    port: row.get(4)?,
                    auto_connect: row.get(5)?,
                })
        }) {
            Ok(r) => {
                match reader::Reader::new_no_repeaters(
                    r.id,
                    r.kind,
                    r.nickname,
                    r.ip_address,
                    r.port,
                    r.auto_connect
                ) {
                    Ok(output) => return Ok(output),
                    Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
                }
            },
            Err(rusqlite::Error::QueryReturnedNoRows) => return Err(DBError::NotFound),
            Err(e) => return Err(DBError::DataRetrievalError(e.to_string())),
        };
    }

    fn get_readers(&self) -> Result<Vec<reader::Reader>, DBError> {
        let mut stmt = match self.conn.prepare("SELECT reader_id, nickname, kind, ip_address, port, auto_connect FROM readers;") {
            Ok(stmt) => stmt,
            Err(e) => return Err(DBError::ConnectionError(e.to_string()))
        };
        let results = match stmt.query_map([],
            |row| {
                Ok(TempReader {
                    id: row.get(0)?,
                    nickname: row.get(1)?,
                    kind: row.get(2)?,
                    ip_address: row.get(3)?,
                    port: row.get(4)?,
                    auto_connect: row.get(5)?
                })
            }) {
                Ok(r) => r,
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            };
        let mut output: Vec<reader::Reader> = Vec::new();
        for row in results {
            match row {
                Ok(r) => {
                    match reader::Reader::new_no_repeaters(
                        r.id,
                        r.kind,
                        r.nickname,
                        r.ip_address,
                        r.port,
                        r.auto_connect
                    ) {
                        Ok(reader) => {
                            output.push(reader);
                        }
                        Err(e) => return Err(e)
                    }
                },
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            }
        }
        Ok(output)
    }

    fn delete_reader(&self, id: &i64) -> Result<usize, DBError> {
        match self.conn.execute("DELETE FROM readers WHERE reader_id=?1", [id]) {
            Ok(num) => return Ok(num),
            Err(e) => return Err(DBError::DataDeletionError(e.to_string()))
        }
    }

    // Results API
    fn save_api(&self, api: &api::Api) -> Result<i64, DBError> {
        match api.kind() {
            api::API_TYPE_CHRONOKEEP_REMOTE |
            api::API_TYPE_CHRONOKEEP_REMOTE_SELF =>
            {},
            _ => return Err(DBError::DataInsertionError(String::from("invalid kind specified")))
        }
        if api.id() > 0 {
            match self.conn.execute(
                "UPDATE results_api SET 
                        nickname=?1, 
                        kind=?2, 
                        token=?3, 
                        uri=?4 
                    WHERE api_id=?5",
                (api.nickname(), api.kind(), api.token(), api.uri(), api.id()))
            {
                Ok(_) => return Ok(api.id()),
                Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
            }
        }
        match self.conn.execute(
            "INSERT INTO results_api (
                    nickname,
                    kind,
                    token,
                    uri
                ) VALUES (?1,?2,?3,?4);",
            (api.nickname(), api.kind(), api.token(), api.uri())
        ) {
            Ok(_) => return Ok(self.conn.last_insert_rowid()),
            Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
        }
    }

    fn get_apis(&self) -> Result<Vec<api::Api>, DBError> {
        let mut stmt = match self.conn.prepare("SELECT api_id, nickname, kind, token, uri FROM results_api;") {
            Ok(stmt) => stmt,
            Err(e) => return Err(DBError::ConnectionError(e.to_string()))
        };
        let results = match stmt.query_map(
            [],
            |row|{
                Ok(api::Api::new(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?
                ))
            }) {
                Ok(r) => r,
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            };
        let mut output: Vec<api::Api> = Vec::new();
        for row in results {
            match row {
                Ok(r) => {
                    match r.kind() {
                        api::API_TYPE_CHRONOKEEP_REMOTE |
                        api::API_TYPE_CHRONOKEEP_REMOTE_SELF =>
                        {
                            output.push(r)
                        },
                        _ => return Err(DBError::DataRetrievalError(String::from("invalid api type")))
                    }
                },
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            }
        }
        return Ok(output);
    }

    fn delete_api(&self, id: &i64) -> Result<usize, DBError> {
        match self.conn.execute("DELETE FROM results_api WHERE api_id=?1", [id]) {
            Ok(num) => return Ok(num),
            Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
        }
    }

    // Reads
    fn save_reads(&mut self, reads: &Vec<read::Read>) -> Result<usize, DBError> {
        if let Ok(tx) = self.conn.transaction() {
            let mut count = 0;
            for r in reads {
                match tx.execute(
                    "INSERT INTO chip_reads (
                            chip, 
                            seconds,
                            milliseconds,
                            reader_seconds,
                            reader_milliseconds,
                            antenna,
                            reader,
                            rssi,
                            uploaded
                        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9);",
                    (r.chip(), r.seconds(), r.milliseconds(), r.reader_seconds(), r.reader_milliseconds(), r.antenna(), r.reader(), r.rssi(), r.uploaded())
                ) {
                    Ok(val) => count = count + val,
                    Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
                }
            }
            if let Err(e) = tx.commit() {
                return Err(DBError::DataInsertionError(e.to_string()));
            }
            return Ok(count);
        }
        return Err(DBError::ConnectionError(String::from("error starting transaction")));
    }

    fn get_reads(&self, start: i64, end: i64) -> Result<Vec<read::Read>, DBError> {
        let mut stmt = match self.conn.prepare("SELECT chip_id, chip, seconds, milliseconds, reader_seconds, reader_milliseconds, antenna, reader, rssi, uploaded FROM chip_reads WHERE seconds >= ?1 AND seconds <= ?2;") {
            Ok(stmt) => stmt,
            Err(e) => return Err(DBError::ConnectionError(e.to_string()))
        };
        let results = match stmt.query_map(
            [start, end],
            |row| {
                Ok(read::Read::new(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            }) {
                Ok(r) => r,
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            };
        let mut output: Vec<read::Read> = Vec::new();
        for row in results {
            match row {
                Ok(r) => {
                    output.push(r);
                },
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            }
        }
        return Ok(output);
    }

    fn get_all_reads(&self) -> Result<Vec<read::Read>, DBError> {
        let mut stmt = match self.conn.prepare("SELECT chip_id, chip, seconds, milliseconds, reader_seconds, reader_milliseconds, antenna, reader, rssi, uploaded FROM chip_reads;") {
            Ok(stmt) => stmt,
            Err(e) => return Err(DBError::ConnectionError(e.to_string()))
        };
        let results = match stmt.query_map(
            [],
            |row| {
                Ok(read::Read::new(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            }) {
                Ok(r) => r,
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            };
        let mut output: Vec<read::Read> = Vec::new();
        for row in results {
            match row {
                Ok(r) => {
                    output.push(r);
                },
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            }
        }
        return Ok(output);
    }

    fn delete_reads(&self, start: i64, end: i64) -> Result<usize, DBError> {
        match self.conn.execute(
            "DELETE FROM chip_reads WHERE seconds >= ?1 AND seconds <= ?2;",
            [start, end]
        ) {
            Ok(num) => return Ok(num),
            Err(e) => return Err(DBError::DataDeletionError(e.to_string()))
        }
    }

    fn delete_all_reads(&self) -> Result<usize, DBError> {
        match self.conn.execute(
            "DELETE FROM chip_reads;",
            []
        ) {
            Ok(num) => return Ok(num),
            Err(e) => return Err(DBError::DataDeletionError(e.to_string()))
        }
    }

    fn get_not_uploaded_reads(&self) -> Result<Vec<read::Read>, DBError> {       
        let mut stmt = match self.conn.prepare("SELECT chip_id, chip, seconds, milliseconds, reader_seconds, reader_milliseconds, antenna, reader, rssi, uploaded FROM chip_reads WHERE uploaded=?1;") {
            Ok(stmt) => stmt,
            Err(e) => return Err(DBError::ConnectionError(e.to_string()))
        };
        let results = match stmt.query_map(
            [read::READ_UPLOADED_FALSE],
            |row| {
                Ok(read::Read::new(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            }) {
                Ok(r) => r,
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            };
        let mut output: Vec<read::Read> = Vec::new();
        for row in results {
            match row {
                Ok(r) => {
                    output.push(r);
                },
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            }
        }
        return Ok(output);
    }

    fn reset_reads_upload(&self) -> Result<usize, DBError> {
        match self.conn.execute(
            "UPDATE chip_reads SET uploaded=?1;",
            [read::READ_UPLOADED_FALSE]
        ) {
            Ok(num) => Ok(num),
            Err(e) => Err(DBError::DataInsertionError(e.to_string()))
        }
    }

    fn update_reads_status(&mut self, reads: &Vec<read::Read>) -> Result<usize, DBError> {
        if let Ok(tx) = self.conn.transaction() {
            let mut count = 0;
            for r in reads {
                match tx.execute(
                    "UPDATE chip_reads SET
                            uploaded=?1
                            WHERE chip_id=?2;",
                    (r.uploaded(), r.id())
                ) {
                    Ok(_) => count += 1,
                    Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
                }
            }
            if let Err(e) = tx.commit() {
                return Err(DBError::DataInsertionError(e.to_string()));
            }
            return Ok(count);
        }
        return Err(DBError::ConnectionError(String::from("error starting transaction")));
    }
}
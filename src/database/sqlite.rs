use crate::objects::{setting, participant, read, sighting};
use crate::network::results;
use crate::database::DBError;
use crate::reader::{self, zebra};

use std::str::FromStr;

#[cfg(test)]
mod tests;

const DATABASE_URI: &str = "./chronokeep-portal.sqlite";

const DATABASE_VERSION_SETTING: &str = "PORTAL_DATABASE_VERSION";
const DATABASE_VERSION: u16 = 1;

pub struct SQLite {
    conn: rusqlite::Connection,
}

struct TempReader {
    id: i64,
    nickname: String,
    kind: String,
    ip_address: String,
    port: u16,
}

impl SQLite {
    pub fn new() -> Result<SQLite, DBError> {
        let new_conn = rusqlite::Connection::open(DATABASE_URI);
        match new_conn {
            Ok(c) => 
                Ok(SQLite {
                    conn: c,
                }),
            Err(e) => Err(DBError::ConnectionError(e.to_string()))
        }
    }

    fn update(&mut self, old_version: u16, new_version: u16) -> Result<(), DBError> {
        if old_version < new_version {
            match old_version {
                1 => {
                    return self.make_tables()
                }
                _ => {
                    return Err(DBError::InvalidVersionError(String::from("invalid version specified for upgrade")))
                }
            }
        } else if new_version < old_version {
            return Err(DBError::DatabaseTooNew(String::from("database version is newer than our known version")))
        }
        return Ok(())
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
                "CREATE TABLE IF NOT EXISTS participants (
                    part_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    bib VARCHAR(50) NOT NULL,
                    first VARCHAR(50) NOT NULL,
                    last VARCHAR(75) NOT NULL,
                    age INTEGER NOT NULL DEFAULT 0,
                    gender VARCHAR(10) NOT NULL DEFAULT 'u',
                    age_group VARCHAR(100) NOT NULL,
                    distance VARCHAR(75) NOT NULL,
                    part_chip VARCHAR(100) NOT NULL UNIQUE,
                    anonymous SMALLINT NOT NULL DEFAULT 0,
                    UNIQUE (bib) ON CONFLICT REPLACE,
                    UNIQUE (part_chip) ON CONFLICT REPLACE
                );",
                "CREATE TABLE IF NOT EXISTS readers (
                    reader_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    nickname VARCHAR(75) NOT NULL,
                    kind VARCHAR(50) NOT NULL,
                    ip_address VARCHAR(100) NOT NULL,
                    port INTEGER NOT NULL,
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
                    status SMALLINT NOT NULL DEFAULT 0,
                    uploaded SMALLINT NOT NULL DEFAULT 0,
                    UNIQUE (chip, seconds, milliseconds) ON CONFLICT IGNORE
                );",
                "CREATE TABLE IF NOT EXISTS sightings (
                    chip_id INTEGER REFERENCES chip_reads(chip_id) ON DELETE CASCADE,
                    part_id INTEGER REFERENCES participants(part_id) ON DELETE CASCADE,
                    UNIQUE (chip_id, part_id) ON CONFLICT IGNORE
                );"
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
        return Err(DBError::ConnectionError(String::from("unable to start transaction")))
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
        match self.conn.query_row("SELECT * FROM settings WHERE setting=?1;",
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
            Err(rusqlite::Error::QueryReturnedNoRows) => return self.make_tables(),
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
        match self.conn.query_row("SELECT * FROM settings WHERE setting=?1;",
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
    fn save_reader(&self, reader: &dyn reader::Reader) -> Result<i64, DBError> {
        match reader.kind() {
            reader::READER_KIND_ZEBRA => {},
            reader::READER_KIND_IMPINJ => return Err(DBError::DataInsertionError(String::from("not yet implemented"))),
            reader::READER_KIND_RFID => return Err(DBError::DataInsertionError(String::from("not yet implemented"))),
            _ => return Err(DBError::DataInsertionError(String::from("unknown reader kind specified")))
        }
        match self.conn.execute(
            "INSERT INTO readers (nickname, kind, ip_address, port) VALUES (?1, ?2, ?3, ?4);",
            (reader.nickname(), reader.kind(), reader.ip_address(), reader.port()),
        ) {
            Ok(_) => return Ok(self.conn.last_insert_rowid()),
            Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
        }
    }

    fn get_reader(&self, id: &i64) -> Result<Box<dyn reader::Reader>, DBError> {
        match self.conn.query_row("SELECT * FROM readers WHERE reader_id=?1;",
            [id],
            |row| {
                Ok(TempReader {
                    id: row.get(0)?,
                    nickname: row.get(1)?,
                    kind: row.get(2)?,
                    ip_address: row.get(3)?,
                    port: row.get(4)?,
                })
        }) {
            Ok(r) => {
                match &r.kind[..] {
                    reader::READER_KIND_ZEBRA => {
                        return Ok(Box::new(
                            zebra::Zebra::new(
                                r.id,
                                r.nickname,
                                r.ip_address,
                                r.port)
                        ))
                    },
                    reader::READER_KIND_IMPINJ => return Err(DBError::DataRetrievalError(String::from("not yet implemented"))),
                    reader::READER_KIND_RFID => return Err(DBError::DataRetrievalError(String::from("not yet implemented"))),
                    _ => return Err(DBError::DataRetrievalError(String::from("unknown reader kind specified")))
                }
            },
            Err(rusqlite::Error::QueryReturnedNoRows) => return Err(DBError::NotFound),
            Err(e) => return Err(DBError::DataRetrievalError(e.to_string())),
        };
    }

    fn get_readers(&self) -> Result<Vec<Box<dyn reader::Reader>>, DBError> {
        let mut stmt = match self.conn.prepare("SELECT * FROM readers;") {
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
                })
            }) {
                Ok(r) => r,
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            };
        let mut output: Vec<Box<dyn reader::Reader>> = Vec::new();
        for row in results {
            match row {
                Ok(r) => {
                    match &r.kind[..] {
                        reader::READER_KIND_ZEBRA => {
                            output.push(Box::new(
                                zebra::Zebra::new(
                                    r.id,
                                    r.nickname,
                                    r.ip_address,
                                    r.port)
                            ))
                        },
                        reader::READER_KIND_IMPINJ => return Err(DBError::DataRetrievalError(String::from("not yet implemented"))),
                        reader::READER_KIND_RFID => return Err(DBError::DataRetrievalError(String::from("not yet implemented"))),
                        _ => return Err(DBError::DataRetrievalError(String::from("unknown reader kind specified")))
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
    fn save_api(&self, api: &results::ResultsApi) -> Result<usize, DBError> {
        match api.kind() {
            results::API_TYPE_CHRONOKEEP | results::API_TYPE_CKEEP_SELF => {},
            _ => return Err(DBError::DataInsertionError(String::from("invalid kind specified")))
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
            Ok(num) => return Ok(num),
            Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
        }
    }

    fn get_apis(&self) -> Result<Vec<results::ResultsApi>, DBError> {
        let mut stmt = match self.conn.prepare("SELECT * FROM results_api;") {
            Ok(stmt) => stmt,
            Err(e) => return Err(DBError::ConnectionError(e.to_string()))
        };
        let results = match stmt.query_map(
            [],
            |row|{
                Ok(results::ResultsApi::new(
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
        let mut output: Vec<results::ResultsApi> = Vec::new();
        for row in results {
            match row {
                Ok(r) => {
                    match r.kind() {
                        results::API_TYPE_CHRONOKEEP | results::API_TYPE_CKEEP_SELF => output.push(r),
                        _ => return Err(DBError::DataRetrievalError(String::from("invalid api type")))
                    }
                },
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            }
        }
        return Ok(output);
    }

    fn delete_api(&self, name: &str) -> Result<usize, DBError> {
        match self.conn.execute("DELETE FROM results_api WHERE nickname=?1", [name]) {
            Ok(num) => return Ok(num),
            Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
        }
    }

    // Reads
    fn save_reads(&mut self, reads: &Vec<read::Read>) -> Result<usize, DBError> {
        if let Ok(tx) = self.conn.transaction() {
            let mut count = 0;
            for r in reads {
                match r.status() {
                    read::READ_STATUS_TOO_SOON | read::READ_STATUS_UNUSED | read::READ_STATUS_USED => {},
                    _ => return Err(DBError::DataInsertionError(String::from("invalid chip read status")))
                }
                match tx.execute(
                    "INSERT INTO chip_reads (
                            chip, 
                            seconds,
                            milliseconds,
                            antenna,
                            reader,
                            rssi,
                            status,
                            uploaded
                        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8);",
                    (r.chip(), r.seconds(), r.milliseconds(), r.antenna(), r.reader(), r.rssi(), r.status(), r.uploaded())
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

    fn get_reads(&self, start: u64, end: u64) -> Result<Vec<read::Read>, DBError> {
        let mut stmt = match self.conn.prepare("SELECT * FROM chip_reads WHERE seconds >= ?1 AND seconds <= ?2;") {
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

    fn delete_reads(&self, start: u64, end: u64) -> Result<usize, DBError> {
        match self.conn.execute(
            "DELETE FROM chip_reads WHERE seconds >= ?1 AND seconds <= ?2;",
            [start, end]
        ) {
            Ok(num) => return Ok(num),
            Err(e) => return Err(DBError::DataDeletionError(e.to_string()))
        }
    }

    // Participants
    fn add_participants(&mut self, participants: &Vec<participant::Participant>) -> Result<usize, DBError> {
        let mut count = 0;
        if let Ok(tx) = self.conn.transaction() {
            for p in participants {
                match tx.execute(
                    "INSERT INTO participants (
                        bib,
                        first,
                        last,
                        age,
                        gender,
                        age_group,
                        distance,
                        part_chip,
                        anonymous
                    ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    (
                        p.bib(),
                        p.first(),
                        p.last(),
                        p.age(),
                        p.gender(),
                        p.age_group(),
                        p.distance(),
                        p.chip(),
                        p.anonymous()
                    )
                ) {
                    Ok(_) => count = count + 1,
                    Err(e) => return Err(DBError::DataInsertionError(e.to_string()))
                }
            }
            if let Err(e) = tx.commit() {
                return Err(DBError::DataInsertionError(e.to_string()))
            }
        }
        Ok(count)
    }

    fn delete_participants(&self) -> Result<usize, DBError> {
        match self.conn.execute(
            "DELETE FROM participants;",
            []
        ) {
            Ok(num) => return Ok(num),
            Err(e) => return Err(DBError::DataDeletionError(e.to_string()))
        }
    }

    fn delete_participant(&self, bib: &str) -> Result<usize, DBError> {
        match self.conn.execute(
            "DELETE FROM participants WHERE bib=?1;",
            [bib]
        ) {
            Ok(num) => return Ok(num),
            Err(e) => return Err(DBError::DataDeletionError(e.to_string()))
        }
    }

    fn get_participants(&self) -> Result<Vec<participant::Participant>, DBError> {
        let mut stmt = match self.conn.prepare("SELECT * FROM participants;") {
            Ok(stmt) => stmt,
            Err(e) => return Err(DBError::ConnectionError(e.to_string()))
        };
        let results = match stmt.query_map(
            [],
            |row| {
                Ok(participant::Participant::new(
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
        let mut output: Vec<participant::Participant> = Vec::new();
        for row in results {
            match row {
                Ok(p) => {
                    output.push(p);
                },
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            }
        }
        return Ok(output);
    }

    fn save_sightings(&mut self, sightings: &Vec<sighting::Sighting>) -> Result<usize, DBError> {
        if let Ok(tx) = self.conn.transaction() {
            let mut count = 0;
            for s in sightings {
                match tx.execute(
                    "INSERT INTO sightings (
                        chip_id,
                        part_id
                    ) VALUES (?1,?2);",
                    (s.read.id(), s.participant.id())
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

    fn get_sightings(&self) -> Result<Vec<sighting::Sighting>, DBError> {
        let mut stmt = match self.conn.prepare(
            "SELECT 
                part_id,
                bib,
                first,
                last,
                age,
                gender,
                age_group,
                distance,
                part_chip,
                anonymous,
                chip_id,
                seconds,
                milliseconds,
                antenna,
                reader,
                rssi,
                status,
                uploaded
            FROM participants NATURAL JOIN sightings NATURAL JOIN chip_reads;"
        ) {
            Ok(stmt) => stmt,
            Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
        };
        let results = match stmt.query_map([],
            |row| {
                Ok(sighting::Sighting{
                    participant: participant::Participant::new(
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
                    ),
                    read: read::Read::new(
                        row.get(10)?,
                        row.get(8)?,
                        row.get(11)?,
                        row.get(12)?,
                        row.get(13)?,
                        row.get(14)?,
                        row.get(15)?,
                        row.get(16)?,
                        row.get(17)?,
                    )
                })
            }
        ) {
            Ok(r) => r,
            Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
        };
        let mut output: Vec<sighting::Sighting> = Vec::new();
        for row in results {
            match row {
                Ok(r) => output.push(r),
                Err(e) => return Err(DBError::DataRetrievalError(e.to_string()))
            }
        }
        return Ok(output);
    }

    fn delete_sightings(&self) -> Result<usize, DBError> {
        match self.conn.execute("DELETE FROM sightings;", []) {
            Ok(num) => return Ok(num),
            Err(e) => return Err(DBError::DataDeletionError(e.to_string()))
        }
    }
}
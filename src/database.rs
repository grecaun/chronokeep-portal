use crate::objects::{setting, participant, read, sighting};
use crate::network::api;
use crate::reader;
use std::fmt;

pub mod sqlite;

#[derive(Debug)]
pub enum DBError {
    ConnectionError(String),
    InvalidVersionError(String),
    DatabaseTooNew(String),
    MutexError(String),
    DataRetrievalError(String),
    DataInsertionError(String),
    DataDeletionError(String),
    NotFound,
}

impl std::error::Error for DBError {}

impl fmt::Display for DBError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DBError::ConnectionError(val) => write!(f, "Connection Error: {val}"),
            DBError::InvalidVersionError(val) => write!(f, "Invalid Database Version: {val}"),
            DBError::DatabaseTooNew(val) => write!(f, "Database Version Too New: {val}"),
            DBError::MutexError(val) => write!(f, "Mutex Error: {val}"),
            DBError::DataRetrievalError(val) => write!(f, "Error Retrieving Data: {val}"),
            DBError::DataInsertionError(val) => write!(f, "Error Inserting Data: {val}"),
            DBError::DataDeletionError(val) => write!(f, "Error Deleting Data: {val}"),
            DBError::NotFound => write!(f, "Data Not Found"),
        }
    }
}

pub trait Database {
    // Setup functions
    fn setup(&mut self) -> Result<(), DBError>;
    // Application settings
    fn set_setting(&self, setting: &setting::Setting) -> Result<setting::Setting, DBError>;
    fn get_setting(&self, name: &str) -> Result<setting::Setting, DBError>;
    // Reader information
    fn save_reader(&self, reader: &reader::Reader) -> Result<i64, DBError>;
    fn get_reader(&self, id: &i64) -> Result<reader::Reader, DBError>;
    fn get_readers(&self) -> Result<Vec<reader::Reader>, DBError>;
    fn delete_reader(&self, id: &i64) -> Result<usize, DBError>;
    // API information
    fn save_api(&self, api: &api::Api) -> Result<i64, DBError>;
    fn get_apis(&self) -> Result<Vec<api::Api>, DBError>;
    fn delete_api(&self, id: &i64) -> Result<usize, DBError>;
    // Information gathered from readers
    fn save_reads(&mut self, reads: &Vec<read::Read>) -> Result<usize, DBError>;
    fn get_reads(&self, start: i64, end: i64) -> Result<Vec<read::Read>, DBError>;
    fn get_all_reads(&self) -> Result<Vec<read::Read>, DBError>;
    fn delete_reads(&self, start: i64, end: i64) -> Result<usize, DBError>;
    fn delete_all_reads(&self) -> Result<usize, DBError>;
    fn reset_reads_status(&self) -> Result<usize, DBError>;
    fn reset_reads_upload(&self) -> Result<usize, DBError>;
    fn get_useful_reads(&self) -> Result<Vec<read::Read>, DBError>;
    fn get_not_uploaded_reads(&self) -> Result<Vec<read::Read>, DBError>;
    fn update_reads_status(&mut self, reads: &Vec<read::Read>) -> Result<usize, DBError>;
    // Participant information
    fn add_participants(&mut self, participants: &Vec<participant::Participant>) -> Result<usize, DBError>;
    fn delete_participants(&self) -> Result<usize, DBError>;
    fn delete_participant(&self, bib: &str) -> Result<usize, DBError>;
    fn get_participants(&self) -> Result<Vec<participant::Participant>, DBError>;
    // Sighting information
    fn save_sightings(&mut self, sightings: &Vec<sighting::Sighting>) -> Result<usize, DBError>;
    fn get_sightings(&self, start: i64, end: i64) -> Result<Vec<sighting::Sighting>, DBError>;
    fn get_all_sightings(&self) -> Result<Vec<sighting::Sighting>, DBError>;
    fn delete_sightings(&self) -> Result<usize, DBError>;
}
use crate::objects::{setting, participant, read};
use crate::network::results;
use crate::reader;
use std::fmt;

pub mod sqlite;

#[derive(Debug)]
pub enum DBError {
    ConnectionError(String),
    InvalidVersionError(String),
    DatabaseTooNew(String),
    MutexError(String),
    RowError(String),
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
            DBError::RowError(val) => write!(f, "Error With Data: {val}"),
            DBError::NotFound => write!(f, "Data Not Found"),
        }
    }
}

pub trait Database {
    // Setup functions
    fn setup(&self) -> Result<(), DBError>;
    // Application settings
    fn set_setting(&self, name: &str, value: &str) -> Result<setting::Setting, DBError>;
    fn get_setting(&self, name: &str) -> Result<setting::Setting, DBError>;
    // Reader information
    fn save_reader(&self, name: &str, kind: &str, ip: &str, port: &u16) -> Result<usize, DBError>;
    fn get_readers(&self) -> Result<Vec<Box<dyn reader::Reader>>, DBError>;
    fn delete_reader(&self, name: &str) -> Result<usize, DBError>;
    // API information
    fn save_api(&self, name: &str, kind: &str, token: &str, uri: &str) -> Result<usize, DBError>;
    fn get_apis(&self) -> Result<Vec<results::ResultsApi>, DBError>;
    fn delete_api(&self, name: &str) -> Result<usize, DBError>;
    // Information gathered from readers
    fn save_reads(&self, reads: Vec<read::Read>) -> Result<usize, DBError>;
    fn get_reads(&self, start: &u64, end: &u64) -> Result<Vec<read::Read>, DBError>;
    fn delete_reads(&self, start: &u64, end: &u64) -> Result<usize, DBError>;
    // Participant information
    fn add_participants(&self, ) -> Result<usize, DBError>;
    fn delete_participants(&self, ) -> Result<usize, DBError>;
    fn delete_participant(&self, bib: &str) -> Result<usize, DBError>;
    fn get_participants(&self, ) -> Result<Vec<participant::Participant>, DBError>;
}
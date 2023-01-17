use crate::objects::{setting, participant, read};
use crate::reader;
use std::fmt;

pub mod sqlite;

#[derive(Debug)]
pub enum DBError {
    ConnectionError,
}

impl std::error::Error for DBError {}

impl fmt::Display for DBError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DBError::ConnectionError => write!(f, "Connection Error"),
        }
    }
}

pub trait Database {
    // Application settings
    fn set_setting(&self, name: &str, value: &str) -> Result<setting::Setting, DBError>;
    fn get_setting(&self, name: &str) -> Result<setting::Setting, DBError>;
    // Reader information
    fn save_reader(&self, name: &str, kind: &str, ip: &str, port: &u16) -> Result<u64, DBError>;
    fn get_readers(&self, ) -> Result<Vec<Box<dyn reader::Reader>>, DBError>;
    fn delete_reader(&self, name: &str) -> Result<u32, DBError>;
    // Information gathered from readers
    fn save_reads(&self, ) -> Result<u32, DBError>;
    fn get_reads(&self, start: &u64, end: &u64) -> Result<Vec<read::Read>, DBError>;
    fn delete_reads(&self, start: &u64, end: &u64) -> Result<u32, DBError>;
    // Participant information
    fn add_participants(&self, ) -> Result<u32, DBError>;
    fn delete_participants(&self, ) -> Result<u32, DBError>;
    fn delete_participant(&self, bib: &str) -> Result<u32, DBError>;
    fn get_participants(&self, ) -> Result<Vec<participant::Participant>, DBError>;
}
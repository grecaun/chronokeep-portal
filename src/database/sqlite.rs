use crate::objects::{setting, participant, read};
use crate::database::DBError;
use crate::reader;

struct SQLite {

}

impl super::Database for SQLite {
    fn set_setting(&self, name: &str, value: &str) -> Result<setting::Setting, DBError> {
        todo!()
    }

    fn get_setting(&self, name: &str) -> Result<setting::Setting, DBError> {
        todo!()
    }

    fn save_reader(&self, name: &str, kind: &str, ip: &str, port: &u16) -> Result<u64, DBError> {
        todo!()
    }

    fn get_readers(&self, ) -> Result<Vec<Box<dyn reader::Reader>>, DBError> {
        todo!()
    }

    fn delete_reader(&self, name: &str) -> Result<u32, DBError> {
        todo!()
    }

    fn save_reads(&self, ) -> Result<u32, DBError> {
        todo!()
    }

    fn get_reads(&self, start: &u64, end: &u64) -> Result<Vec<read::Read>, DBError> {
        todo!()
    }

    fn delete_reads(&self, start: &u64, end: &u64) -> Result<u32, DBError> {
        todo!()
    }

    fn add_participants(&self, ) -> Result<u32, DBError> {
        todo!()
    }

    fn delete_participants(&self, ) -> Result<u32, DBError> {
        todo!()
    }

    fn delete_participant(&self, bib: &str) -> Result<u32, DBError> {
        todo!()
    }

    fn get_participants(&self, ) -> Result<Vec<participant::Participant>, DBError> {
        todo!()
    }
}
use std::{thread::JoinHandle, sync};

use crate::database::sqlite::SQLite;

pub struct TestReader {
    id: i64,
    nickname: String,
    kind: String,
    ip_address: String,
    port: u16,
}

impl TestReader {
    pub fn new(
        nickname: String,
        kind: String,
        ip_address: String,
        port: u16
    ) -> TestReader {
        TestReader {
            id: 0,
            nickname,
            kind,
            ip_address,
            port,
        }
    }
}

impl crate::reader::Reader for TestReader {
    fn set_id(&mut self, id: i64) {
        self.id = id;
    }

    fn id(&self) -> i64 {
        self.id
    }
    
    fn nickname(&self) -> &str {
        &self.nickname
    }

    fn kind(&self) -> &str{
        &self.kind
    }

    fn ip_address(&self) -> &str {
        &self.ip_address
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn equal(&self, other: &dyn super::Reader) -> bool {
        self.nickname == other.nickname() &&
            self.kind == other.kind() &&
            self.ip_address == other.ip_address() &&
            self.port == other.port()
    }

    fn connect(&mut self) -> Result<JoinHandle<()>, &'static str> {
        todo!()
    }

    fn disconnect(&mut self) -> Result<(), &'static str> {
        todo!()
    }

    fn initialize(&mut self) -> Result<(), &'static str> {
        todo!()
    }

    fn send(&mut self, _buf: &[u8]) -> Result<(), &'static str> {
        todo!()
    }

    fn get_next_id(&mut self) -> u32 {
        todo!()
    }

    fn stop(&mut self) -> Result<(), &'static str> {
        todo!()
    }
}
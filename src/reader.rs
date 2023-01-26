use std::thread::JoinHandle;

pub mod llrp;

pub const READER_KIND_LLRP: &str = "LLRP";
pub const READER_KIND_RFID: &str = "RFID";
pub const READER_KIND_IMPINJ: &str = "IMPINJ";

pub trait Reader {
    // get functions for fields
    fn set_id(&mut self, id: i64);
    fn id(&self) -> i64;
    fn nickname(&self) -> &str;
    fn kind(&self) -> &str;
    fn ip_address(&self) -> &str;
    fn port(&self) -> u16;
    fn equal(&self, other: &dyn Reader) -> bool;
    fn set_time(&self) -> Result<(), &'static str>;
    fn get_time(&self) -> Result<(), &'static str>;
    fn connect(&mut self) -> Result<JoinHandle<()>, &'static str>;
    fn disconnect(&mut self) -> Result<(), &'static str>;
    fn initialize(&self) -> Result<(), &'static str>;
    fn send(&mut self, buf: &[u8]) -> Result<(), &'static str>;
    fn get_next_id(&mut self) -> u32;
}
pub mod zebra;

pub const READER_KIND_ZEBRA: &str = "ZEBRA";
pub const READER_KIND_RFID: &str = "RFID";
pub const READER_KIND_IMPINJ: &str = "IMPINJ";

pub trait Reader {
    fn get_kind(&self);
    fn get_connected(&self);

    fn process_messages(&self);
    fn set_time(&self);
    fn get_time(&self);
    fn connect(&self);
    fn initialize(&self);
}
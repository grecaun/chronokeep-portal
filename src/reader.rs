pub mod zebra;

pub const READER_KIND_ZEBRA: &str = "ZEBRA";
pub const READER_KIND_RFID: &str = "RFID";
pub const READER_KIND_IMPINJ: &str = "IMPINJ";

pub trait Reader {
    // get functions for fields
    fn id(&self) -> usize;
    fn nickname(&self) -> &str;
    fn kind(&self) -> &str;
    fn ip_address(&self) -> &str;
    fn port(&self) -> u16;
    fn connected_at(&self) -> &str;
    fn is_connected(&self) -> bool;
    fn equal(&self, other: &dyn Reader) -> bool;
    fn process_messages(&self);
    fn set_time(&self);
    fn get_time(&self);
    fn connect(&self);
    fn initialize(&self);
}
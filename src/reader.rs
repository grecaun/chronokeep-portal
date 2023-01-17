pub mod zebra;

pub trait Reader {
    fn get_kind(&self);
    fn get_connected(&self);

    fn process_messages(&self);
    fn set_time(&self);
    fn get_time(&self);
    fn connect(&self);
    fn initialize(&self);
}
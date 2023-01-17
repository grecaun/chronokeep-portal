pub struct Zebra {
    kind: String,
    name: String,
    ip_address: String,
    port: u16,
    connected: bool,
    connected_at: String,
    // list of sockets to be connected to
}

impl Zebra {
    fn new(name: String, kind: String, ip_address: String, port: u16) -> Zebra {
        Zebra {
            kind,
            name,
            ip_address,
            port,
            connected: false,
            connected_at: String::from(""),
        }
    }
}

impl super::Reader for Zebra {
    fn get_kind(&self) {
        todo!()
    }

    fn get_connected(&self) {
        todo!()
    }

    fn process_messages(&self) {
        todo!()
    }

    fn set_time(&self) {
        todo!()
    }

    fn get_time(&self) {
        todo!()
    }

    fn connect(&self) {
        todo!()
    }

    fn initialize(&self) {
        todo!()
    }
}
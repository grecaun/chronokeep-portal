pub struct Zebra {
    id: usize,
    nickname: String,
    kind: String,
    ip_address: String,
    port: u16,
    connected: bool,
    connected_at: String,
    // list of sockets to be connected to
}

impl Zebra {
    pub fn new(
        id: usize,
        nickname: String,
        kind: String,
        ip_address: String,
        port: u16
    ) -> Zebra {
        Zebra {
            id,
            kind,
            nickname,
            ip_address,
            port,
            connected: false,
            connected_at: String::from(""),
        }
    }
}

impl super::Reader for Zebra {
    fn id(&self) -> usize {
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

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn connected_at(&self) -> &str {
        &self.connected_at
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
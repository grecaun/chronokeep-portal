pub struct TestReader {
    id: usize,
    nickname: String,
    kind: String,
    ip_address: String,
    port: u16,
    connected: bool,
    connected_at: String,
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
            connected: false,
            connected_at: String::from("")
        }
    }
}

impl super::Reader for TestReader {
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

    fn equal(&self, other: &dyn super::Reader) -> bool {
        self.nickname == other.nickname() &&
            self.kind == other.kind() &&
            self.ip_address == other.ip_address() &&
            self.port == other.port()
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
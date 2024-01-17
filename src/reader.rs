use std::{thread::JoinHandle, sync::{Mutex, Arc, self, Condvar}, net::TcpStream, io::Write};

use serde::{Deserialize, Serialize};

use crate::{database::{sqlite, DBError}, control::{self, socket::MAX_CONNECTED}, processor};

pub mod zebra;
pub mod auto_connect;

pub const READER_KIND_ZEBRA: &str = "ZEBRA";
pub const READER_KIND_RFID: &str = "RFID";
pub const READER_KIND_IMPINJ: &str = "IMPINJ";

pub const AUTO_CONNECT_TRUE: u8 = 1;
pub const AUTO_CONNECT_FALSE: u8 = 0;

#[derive(Serialize, Deserialize)]
pub struct Reader {
    id: i64,
    nickname: String,
    kind: String,
    ip_address: String,
    port: u16,
    auto_connect: u8,

    #[serde(skip)]
    pub socket: sync::Mutex<Option<TcpStream>>,
    #[serde(skip)]
    pub keepalive: Arc<sync::Mutex<bool>>,
    #[serde(skip)]
    pub msg_id: Arc<sync::Mutex<u32>>,

    #[serde(skip)]
    pub reading: Arc<sync::Mutex<bool>>,
    #[serde(skip)]
    pub connected: Arc<sync::Mutex<bool>>,
    
    #[serde(skip)]
    control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
    #[serde(skip)]
    read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
    #[serde(skip)]
    sight_processor: Option<Arc<processor::SightingsProcessor>>,
}

impl Reader {
    pub(crate) fn new_internal(
        id: i64,
        kind: String,
        nickname: String,
        ip_address: String,
        port: u16,
        auto_connect: u8,
    ) -> Reader {
        Reader {
            id,
            kind,
            nickname,
            ip_address,
            port,
            socket: Mutex::new(None),
            keepalive: Arc::new(Mutex::new(true)),
            msg_id: Arc::new(Mutex::new(0)),
            reading: Arc::new(Mutex::new(false)),
            connected: Arc::new(Mutex::new(false)),
            auto_connect,
            control_sockets: Arc::new(Mutex::new(Default::default())),
            read_repeaters: Arc::new(Mutex::new(Default::default())),
            sight_processor: None,
        }
    }

    pub fn new_no_repeaters(
        id: i64,
        kind: String,
        nickname: String,
        ip_address: String,
        port: u16,
        auto_connect: u8,
    ) -> Result<Reader, DBError> {
        match kind.as_str() {
            READER_KIND_ZEBRA => {
                return Ok(Reader::new_internal(id, kind, nickname, ip_address, port, auto_connect))
            },
            READER_KIND_IMPINJ => return Err(DBError::DataRetrievalError(String::from("not yet implemented"))),
            READER_KIND_RFID => return Err(DBError::DataRetrievalError(String::from("not yet implemented"))),
            _ => return Err(DBError::DataRetrievalError(String::from("unknown reader kind specified")))
        }
    }

    pub fn new(
        id: i64,
        kind: String,
        nickname: String,
        ip_address: String,
        port: u16,
        auto_connect: u8,
        control_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>,
        read_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>,
        sight_processor: Arc<processor::SightingsProcessor>,
    ) -> Result<Reader, DBError> {
        match kind.as_str() {
            READER_KIND_ZEBRA => {
                Ok(Reader {
                    id,
                    kind,
                    nickname,
                    ip_address,
                    port,
                    socket: sync::Mutex::new(None),
                    keepalive: Arc::new(sync::Mutex::new(true)),
                    msg_id: Arc::new(sync::Mutex::new(0)),
                    reading: Arc::new(sync::Mutex::new(false)),
                    connected: Arc::new(sync::Mutex::new(false)),
                    auto_connect,
                    control_sockets,
                    read_repeaters,
                    sight_processor: Some(sight_processor),
                })
            },
            READER_KIND_IMPINJ => return Err(DBError::DataRetrievalError(String::from("not yet implemented"))),
            READER_KIND_RFID => return Err(DBError::DataRetrievalError(String::from("not yet implemented"))),
            _ => return Err(DBError::DataRetrievalError(String::from("unknown reader kind specified")))
        }
    }
    
    pub fn set_id(&mut self, id: i64) {
        self.id = id
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn set_nickname(&mut self, name: String) {
        self.nickname = name
    }

    pub fn nickname(&self) -> &str {
        self.nickname.as_str()
    }
    
    pub fn set_kind(&mut self, kind: String) {
        self.kind = kind
    }

    pub fn kind(&self) -> &str {
        self.kind.as_str()
    }

    pub fn set_ip_address(&mut self, ip_address: String) {
        self.ip_address = ip_address
    }

    pub fn ip_address(&self) -> &str {
        self.ip_address.as_str()
    }

    pub fn set_port(&mut self, port: u16) {
        self.port = port
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn set_auto_connect(&mut self, ac: u8) {
        self.auto_connect = ac
    }

    pub fn auto_connect(&self) -> u8 {
        self.auto_connect
    }

    pub fn set_control_sockets(&mut self, c_sockets: Arc<Mutex<[Option<TcpStream>;MAX_CONNECTED + 1]>>) {
        self.control_sockets = c_sockets
    }

    pub fn set_read_repeaters(&mut self, r_repeaters: Arc<Mutex<[bool;MAX_CONNECTED]>>) {
        self.read_repeaters = r_repeaters
    }

    pub fn set_sight_processor(&mut self, s_processor: Arc<processor::SightingsProcessor>) {
        self.sight_processor = Some(s_processor)
    }

    pub fn equal(&self, other: &Reader) -> bool {
        self.nickname == other.nickname() &&
            self.kind == other.kind() &&
            self.ip_address == other.ip_address() &&
            self.port == other.port()
    }

    pub fn is_connected(&self) -> Option<bool> {
        let mut output: Option<bool> = None;
        if let Ok(con) = self.connected.lock() {
            output = Some(*con);
        }
        output
    }

    pub fn is_reading(&self) -> Option<bool> {
        let mut output: Option<bool> = None;
        if let Ok(con) = self.reading.lock() {
            output = Some(*con);
        }
        output
    }

    pub fn disconnect(&mut self) -> Result<(), &'static str> {
        _ = self.stop();
        if let Ok(mut keepalive) = self.keepalive.lock() {
            *keepalive = false;
        };
        if let Ok(mut con) = self.connected.lock() {
            *con = false;
        }
        Ok(())
    }

    pub fn send(&mut self, buf: &[u8]) -> Result<(), &'static str> {
        if let Ok(stream) = self.socket.lock() {
            match &*stream {
                Some(s) => {
                    let mut w_stream = match s.try_clone() {
                        Ok(v) => v,
                        Err(_) => return Err("unable to copy stream")
                    };
                    match w_stream.write_all(buf) {
                        Ok(_) => (),
                        Err(_) => return Err("error writing data")
                    }
                    Ok(())
                },
                None => {
                    Err("not connected")
                },
            }
        } else {
            Err("unable to get mutex")
        }
    }
    
    pub fn get_next_id(&mut self) -> u32 {
        let mut output: u32 = 0;
        if let Ok(mut v) = self.msg_id.lock() {
            output = *v + 1;
            *v = output;
        }
        output
    }

    pub fn connect(&mut self, sqlite: &Arc<Mutex<sqlite::SQLite>>, controls: &Arc<Mutex<control::Control>>, sound_notifier: Arc<Condvar>) -> Result<JoinHandle<()>, &'static str> {
        match self.kind.as_str() {
            READER_KIND_ZEBRA => {
                zebra::connect(self, sqlite, controls, sound_notifier)
            }
            _ => {
                Err("reader type not supported")
            }
        }
    }

    pub fn initialize(&mut self) -> Result<(), &'static str> {
        match self.kind.as_str() {
            READER_KIND_ZEBRA => {
                zebra::initialize(self)
            }
            _ => {
                Err("reader type not supported")
            }
        }
    }

    pub fn stop(&mut self) -> Result<(), &'static str>  {
        match self.kind.as_str() {
            READER_KIND_ZEBRA => {
                zebra::stop_reader(self)
            }
            _ => {
                Err("reader type not supported")
            }
        }
    }
}
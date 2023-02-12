use serde::Serialize;

use crate::{objects::{setting, participant::Participant}, network::api};

#[derive(Serialize, Debug)]
#[serde(tag="command", rename_all="snake_case")]
pub enum Responses {
    Readers {
        readers: Vec<Reader>,
    },
    Error {
        message: String,
    },
    Settings {
        settings: Vec<setting::Setting>,
    },
    ApiList {
        apis: Vec<api::Api>,
    },
    Reads {
        list: Vec<Read>,
    },
    Success {
        count: usize,
    },
    Time {
        local: String,
        utc: String,
    },
    Participants {
        participants: Vec<Participant>,
    },
    ConnectionSuccessful {
        kind: String,
        version: usize,
    },
    Keepalive
}

#[derive(Serialize, Debug)]
#[serde(rename_all="snake_case")]
pub struct Reader {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub ip_address: String,
    pub port: u16,
    pub reading: Option<bool>,
    pub connected: Option<bool>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all="snake_case")]
pub struct Read {
    pub id: u64,
    pub chip: String,
    pub seconds: u64,
    pub milliseconds: u32,
    pub antenna: u32,
    pub reader: String,
    pub rssi: String
}
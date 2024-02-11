use serde::Serialize;

use crate::{network::api, objects::{event::Event, participant::Participant, read, setting, sighting::Sighting}, reader::MAX_ANTENNAS, remote::uploader};

use super::errors;

#[derive(Serialize, Debug)]
#[serde(tag="command", rename_all="snake_case")]
pub enum Responses {
    Readers {
        readers: Vec<Reader>,
    },
    ReaderAntennas{
        reader_name: String,
        antennas: [u8;MAX_ANTENNAS],
    },
    Error {
        error: errors::Errors,
    },
    Settings {
        settings: Vec<setting::Setting>,
    },
    SettingsAll {
        settings: Vec<setting::Setting>,
        readers: Vec<Reader>,
        apis: Vec<api::Api>,
        auto_upload: uploader::Status,
    },
    ApiList {
        apis: Vec<api::Api>,
    },
    Reads {
        list: Vec<read::Read>,
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
    Sightings {
        list: Vec<Sighting>,
    },
    Events {
        events: Vec<Event>,
    },
    EventYears {
        years: Vec<String>
    },
    ReadAutoUpload {
        status: uploader::Status,
    },
    ConnectionSuccessful {
        name: String,
        kind: String,
        version: usize,
        reads_subscribed: bool,
        sightings_subscribed: bool,
        readers: Vec<Reader>,
        updatable: bool
    },
    Keepalive,
    Disconnect,
}

#[derive(Serialize, Debug)]
#[serde(rename_all="snake_case")]
pub struct Reader {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub ip_address: String,
    pub port: u16,
    pub auto_connect: bool,
    pub reading: Option<bool>,
    pub connected: Option<bool>,
    pub antennas: [u8;MAX_ANTENNAS],
}
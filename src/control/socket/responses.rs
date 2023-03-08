use serde::Serialize;

use crate::{objects::{event::Event, setting, participant::Participant, sighting::Sighting, read}, network::api};

use super::errors;

#[derive(Serialize, Debug)]
#[serde(tag="command", rename_all="snake_case")]
pub enum Responses {
    Readers {
        readers: Vec<Reader>,
    },
    Error {
        error: errors::Errors,
    },
    Settings {
        settings: Vec<setting::Setting>,
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
    ConnectionSuccessful {
        name: String,
        kind: String,
        version: usize,
        reads_subscribed: bool,
        sightings_subscribed: bool,
        readers: Vec<Reader>
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
    pub reading: Option<bool>,
    pub connected: Option<bool>,
}
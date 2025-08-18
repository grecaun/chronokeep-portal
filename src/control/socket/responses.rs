use serde::Serialize;

use crate::{network::api, objects::{read, setting}, reader::MAX_ANTENNAS, remote::uploader};

use super::{errors, notifications};

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
    Notification {
        kind: notifications::APINotification,
        time: String,
    },
    Settings {
        settings: Vec<setting::Setting>,
    },
    SettingsAll {
        settings: Vec<setting::Setting>,
        readers: Vec<Reader>,
        apis: Vec<api::Api>,
        auto_upload: uploader::Status,
        portal_version: &'static str,
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
    ReadAutoUpload {
        status: uploader::Status,
    },
    ConnectionSuccessful {
        name: String,
        kind: String,
        version: usize,
        reads_subscribed: bool,
        readers: Vec<Reader>,
        updatable: bool,
        auto_upload: uploader::Status,
        portal_version: &'static str,
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
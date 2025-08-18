use serde::Deserialize;

use crate::{network::api, objects::{read, setting::Setting}};

use super::notifications;

#[derive(Deserialize, Debug)]
#[serde(tag="command", rename_all="snake_case")]
pub enum Request {
    Unknown,
    // Api related requests
    ApiSave {
        id: i64,
        name: String,
        kind: String,
        uri: String,
        token: String,
    },
    ApiSaveAll{
        list: Vec<api::Api>,
    },
    ApiList,
    ApiRemoteAutoUpload {
        query: AutoUploadQuery,
    },
    ApiRemoteManualUpload,
    ApiRemove {
        id: i64,
    },
    // Connection or program related requests
    Connect {
        reads: bool,
    },
    Disconnect,
    KeepaliveAck,
    Quit,
    Shutdown,
    Restart,
    // Reader related requests
    ReaderAdd {
        id: i64,
        name: String,
        kind: String,
        ip_address: String,
        port: u16,
        auto_connect: bool,
    },
    ReaderConnect {
        id: i64,
    },
    ReaderDisconnect {
        id: i64,
    },
    ReaderList,
    ReaderRemove {
        id: i64,
    },
    ReaderStart {
        id: i64,
    },
    ReaderStop {
        id: i64,
    },
    ReaderStartAll,
    ReaderStopAll,
    ReaderGetAll,
    // Reads related requests
    ReadsAdd {
        read: read::Read
    },
    ReadsDeleteAll,
    ReadsDelete {
        start_seconds: i64,
        end_seconds: i64,
    },
    ReadsGetAll,
    ReadsGet {
        start_seconds: i64,
        end_seconds: i64,
    },
    // Settings related requests
    SettingsSet {
        settings: Vec<Setting>
    },
    SettingsGet,
    SettingsGetAll,
    // Subscription request to subscribe to new reads/sightings.
    Subscribe {
        reads: bool,
    },
    // Time related requests
    TimeGet,
    TimeSet {
        time: String,
    },
    // Request to update the software.
    Update,
    SetNoficiation {
        kind: notifications::APINotification,
    },
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="snake_case")]
pub enum AutoUploadQuery {
    Stop,
    Start,
    Status,
}
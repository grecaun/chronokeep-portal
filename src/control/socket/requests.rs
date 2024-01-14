use serde::Deserialize;

use crate::{objects::{setting::Setting, read, participant::Participant}, network::api};

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
    ApiResultsEventsGet {
        api_id: i64,
    },
    ApiResultsEventYearsGet {
        api_id: i64,
        event_slug: String,
    },
    ApiResultsParticipantsGet {
        api_id: i64,
        event_slug: String,
        event_year: String,
    },
    // Connection or program related requests
    Connect {
        reads: bool,
        sightings: bool,
    },
    Disconnect,
    KeepaliveAck,
    Quit,
    Shutdown,
    // Participants related requests
    ParticipantsGet,
    ParticipantsRemove,
    ParticipantsAdd {
        participants: Vec<Participant>,
    },
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
    SightingsGetAll,
    SightingsGet {
        start_seconds: i64,
        end_seconds: i64,
    },
    SightingsDelete,
    // Settings related requests
    SettingsSet {
        settings: Vec<Setting>
    },
    SettingsGet,
    SettingsGetAll,
    // Subscription request to subscribe to new reads/sightings.
    Subscribe {
        reads: bool,
        sightings: bool,
    },
    // Time related requests
    TimeGet,
    TimeSet {
        time: String,
    },
    // Request to update the software.
    Update,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="snake_case")]
pub enum AutoUploadQuery {
    Stop,
    Start,
    Status,
}
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(tag="command", rename_all="snake_case")]
pub enum Request {
    Unknown,
    Connect,
    Disconnect,
    KeepaliveAck,
    ReaderList,
    ReaderAdd {
        name: String,
        kind: String,
        ip_address: String,
        port: u16,
    },
    ReaderRemove {
        id: i64,
    },
    ReaderConnect {
        id: i64,
    },
    ReaderDisconnect {
        id: i64,
    },
    ReaderStart {
        id: i64,
    },
    ReaderStop {
        id: i64,
    },
    SettingsGet,
    SettingSet {
        name: String,
        value: String,
    },
    Quit,
    ApiList,
    ApiAdd {
        name: String,
        kind: String,
        uri: String,
        token: String,
    },
    ApiRemove {
        name: String,
    },
    ApiRemoteManualUpload {
        name: String,
    },
    ApiRemoteAutoUpload {
        name: String,
    },
    ApiResultsEventsGet {
        name: String,
    },
    ApiResultsParticipantsGet {
        api_name: String,
        event_slug: String,
        event_year: String,
    },
    ParticipantsGet,
    ParticipantsRemove,
    ReadsGet {
        start_seconds: u64,
        end_seconds: u64,
    },
    ReadsGetAll,
    ReadsDelete {
        start_seconds: u64,
        end_seconds: u64,
    },
    ReadsDeleteAll,
    TimeGet,
    TimeSet {
        time: String,
    },
    Subscribe {
        reads: bool,
        sightings: bool,
    }
}
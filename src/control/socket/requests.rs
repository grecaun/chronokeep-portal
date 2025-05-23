use serde::{Deserialize, Serialize};

use crate::{network::api, objects::{bibchip::BibChip, participant, read, setting::Setting}};

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
    Restart,
    // Participants related requests
    ParticipantsGet,
    ParticipantsRemove,
    ParticipantsAdd {
        participants: Vec<RequestParticipant>,
    },
    // BibChip related requests
    BibChipsGet,
    BibChipsRemove,
    BibChipsAdd {
        bib_chips: Vec<BibChip>,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all="snake_case")]
pub struct RequestParticipant {
    id: String,
    bib: String,
    first: String,
    last: String,
    birthdate: String,
    gender: String,
    age_group: String,
    distance: String,
    anonymous: bool,
    sms_enabled: bool,
    mobile: String,
    apparel: String,
}

impl RequestParticipant {
    pub fn get_participant(&self) -> participant::Participant {
        participant::Participant::new(
            0,
            self.bib.clone(),
            self.first.clone(),
            self.last.clone(),
            self.birthdate.clone(),
            self.gender.clone(),
            self.age_group.clone(),
            self.distance.clone(),
            self.anonymous
        )
    }
}
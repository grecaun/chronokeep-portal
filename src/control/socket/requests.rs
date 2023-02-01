use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(tag="command", rename_all="camelCase")]
pub enum Request {
    Unknown,
    ReaderList,
    ReaderAdd {
        details: ReaderAddDetails,
    },
    ReaderRemove {
        details: ReaderDetails,
    },
    ReaderConnect {
        details: ReaderDetails,
    },
    ReaderDisconnect {
        details: ReaderDetails,
    },
    ReaderStart {
        details: ReaderDetails,
    },
    ReaderStop {
        details: ReaderDetails,
    },
    SettingsGet,
    SetingSet {
        details: SettingDetail,
    },
    Quit,
    ApiList,
    ApiAdd {
        details: ApiAddDetail,
    },
    ApiRemove {
        details: ApiDetail,
    },
    ApiRemoteManualUpload {
        details: ApiDetail,
    },
    ApiRemoteAutoUpload {
        details: ApiDetail,
    },
    ApiResultsEventsGet {
        details: ApiDetail,
    },
    ApiResultsParticipantsGet {
        details: ApiResultsParticipantsGetDetail
    },
    ApiParticipantsRemove,
    ReadsGet {
        details: ReadsDetail,
    },
    ReadsGetAll,
    ReadsDelete {
        details: ReadsDetail,
    },
    ReadsDeleteAll,
    TimeGet,
    TimeSet {
        details: TimeDetail,
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReaderAddDetails {}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReaderDetails {}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SettingDetail {}


#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ApiAddDetail {}
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ApiDetail {}
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ApiResultsParticipantsGetDetail {}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReadsDetail {}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TimeDetail {}
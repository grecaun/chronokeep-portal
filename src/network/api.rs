use serde::Serialize;

pub const API_TYPE_CHRONOKEEP_RESULTS: &str = "CHRONOKEEP_RESULTS";
pub const API_TYPE_CKEEP_RESULTS_SELF: &str = "CHRONOKEEP_RESULTS_SELF";
pub const API_TYPE_CHRONOKEEP_REMOTE: &str = "CHRONOKEEP_REMOTE";
pub const API_TYPE_CKEEP_REMOTE_SELF: &str = "CHRONOKEEP_REMOTE_SELF";

pub const API_URI_CHRONOKEEP_RESULTS: &str = "https://api.chronokeep.com/";
pub const API_URI_CHRONOKEEP_REMOTE: &str = "https://remote.chronokeep.com/";

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all="camelCase")]
pub struct Api {
    id: usize,
    nickname: String,
    kind: String,
    token: String,
    uri: String,
}

impl Api {
    pub fn new(
        id: usize,
        nickname: String,
        kind: String,
        token: String,
        uri: String) -> Api {
        Api {
            id,
            nickname,
            kind,
            token,
            uri,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn nickname(&self) -> &str {
        &self.nickname
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn equal(&self, other: &Api) -> bool {
        self.nickname == other.nickname &&
            self.kind == other.kind &&
            self.token == other.token &&
            self.uri == other.uri
    } 
}
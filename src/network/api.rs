use serde::{Serialize, Deserialize};

pub const API_TYPE_CHRONOKEEP_REMOTE: &str = "CHRONOKEEP_REMOTE";
pub const API_TYPE_CHRONOKEEP_REMOTE_SELF: &str = "CHRONOKEEP_REMOTE_SELF";

pub const API_URI_CHRONOKEEP_REMOTE: &str = "https://remote.chronokeep.com/";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all="camelCase")]
pub struct Api {
    id: i64,
    nickname: String,
    kind: String,
    token: String,
    uri: String,
}

impl Api {
    pub fn new(
        id: i64,
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

    pub fn id(&self) -> i64 {
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
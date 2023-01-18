//const API_TYPE_CHRONOKEEP: &str = "CHRONOKEEP";
//const API_TYPE_CKEEP_SELF: &str = "CHRONOKEEP_SELF";

pub struct ResultsApi {
    id: usize,
    nickname: String,
    kind: String,
    token: String,
    uri: String,
}

impl ResultsApi {
    pub fn new(
        id: usize,
        nickname: String,
        kind: String,
        token: String,
        uri: String) -> ResultsApi {
        ResultsApi {
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
}
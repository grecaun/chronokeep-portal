const API_TYPE_CHRONOKEEP: &str = "CHRONOKEEP";
const API_TYPE_CKEEP_SELF: &str = "CHRONOKEEP_SELF";

pub struct ResultsApi {
    id: usize,
    nickname: String,
    kind: String,
    token: String,
    uri: String,
}

impl ResultsApi {
    pub fn new(id: usize, nickname: String, kind: String, token: String, uri: String) -> ResultsApi {
        ResultsApi {
            id,
            nickname,
            kind,
            token,
            uri,
        }
    }
}
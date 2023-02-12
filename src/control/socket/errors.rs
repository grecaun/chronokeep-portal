use serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(tag="error_type", rename_all="snake_case")]
pub enum Errors {
    UnknownCommand,
    TooManyConnections,
    ServerError{
        message: String,
    },
    DatabaseError{
        message: String,
    },
    InvalidReaderType {
        message: String,
    },
    ReaderConnection {
        message: String,
    },
    NotFound,
    InvalidSetting {
        message: String,
    },
    InvalidApiType {
        message: String,
    },
    AlreadySubscribed {
        message: String,
    },
}
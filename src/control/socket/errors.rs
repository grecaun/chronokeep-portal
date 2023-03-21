use serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(tag="error_type", rename_all="SCREAMING_SNAKE_CASE")]
pub enum Errors {
    UnknownCommand,
    TooManyConnections,
    TooManyRemoteApi,
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
    AlreadyRunning,
    NotRunning,
    NoRemoteApi,
    StartingUp,
    InvalidRead,
}
// Protocol Version Management
pub const GET_SUPPORTED_VERSION: u16 = 46;
pub const GET_SUPPORTED_VERSION_RESPONSE: u16 = 56;
pub const SET_PROTOCOL_VERSION: u16 = 47;
pub const SET_PROTOCOL_VERSION_RESPONSE: u16 = 57;

// Reader Device Capabilities
pub const GET_READER_CAPABILITIES: u16 = 1;
pub const GET_READER_CAPABILITIES_RESPONSE: u16 = 11;

// Reader Operations (RO)
pub const ADD_ROSPEC: u16 = 20;
pub const ADD_ROSPEC_RESPONSE: u16 = 30;
pub const DELETE_ROSPEC: u16 = 21;
pub const DELETE_ROSPEC_RESPONSE: u16 = 31;
pub const START_ROSPEC: u16 = 22;
pub const START_ROSPEC_RESPONSE: u16 = 32;
pub const STOP_ROSPEC: u16 = 23;
pub const STOP_ROSPEC_RESPONSE: u16 = 33;
pub const ENABLE_ROSPEC: u16 = 24;
pub const ENABLE_ROSPEC_RESPONSE: u16 = 34;
pub const DISABLE_ROSPEC: u16 = 25;
pub const DISABLE_ROSPEC_RESPONSE: u16 = 35;
pub const GET_ROSPECS: u16 = 26;
pub const GET_ROSPECS_RESPONSE: u16 = 36;

// Access Operation
pub const ADD_ACCESS_SPEC: u16 = 40;
pub const ADD_ACCESS_SPEC_RESPONSE: u16 = 50;
pub const DELETE_ACCESS_SPEC: u16 = 41;
pub const DELETE_ACCESS_SPEC_RESPONSE: u16 = 51;
pub const ENABLE_ACCESS_SPEC: u16 = 42;
pub const ENABLE_ACCESS_SPEC_RESPONSE: u16 = 52;
pub const DISABLE_ACCESS_SPEC: u16 = 43;
pub const DISABLE_ACCESS_SPEC_RESPONSE: u16 = 53;
pub const GET_ACCESS_SPECS: u16 = 44;
pub const GET_ACCESS_SPECS_RESPONSE: u16 = 54;
pub const CLIENT_REQUEST_OP: u16 = 45;
pub const CLIENT_REQUEST_OP_RESPONSE: u16 = 55;

// Reader Device Configuration
pub const GET_READER_CONFIG: u16 = 2;
pub const GET_READER_CONFIG_RESPONSE: u16 = 12;
pub const SET_READER_CONFIG: u16 = 3;
pub const SET_READER_CONFIG_RESPONSE: u16 = 13;
pub const CLOSE_CONNECTION: u16 = 14;
pub const CLOSE_CONNECTION_RESPONSE: u16 = 4;

// Reports, Notifications and Keepalives
pub const GET_REPORT: u16 = 60;
pub const RO_ACCESS_REPORT: u16 = 61;
pub const KEEPALIVE: u16 = 62;
pub const KEEPALIVE_ACK: u16 = 72;
pub const READER_EVENT_NOTIFICATION: u16 = 63;
pub const ENABLE_EVENTS_AND_REPORTS: u16 = 64;

// Errors
pub const ERROR_MESSAGE: u16 = 100;

// Custom Message
pub const CUSTOM_MESSAGE: u16 = 1023;

pub fn get_message_name(kind: u16) -> Option<&'static str> {
    match kind {
        46 => Some("GET_SUPPORTED_VERSION"),
        56 => Some("GET_SUPPORTED_VERSION_RESPONSE"),
        47 => Some("SET_PROTOCOL_VERSION"),
        57 => Some("SET_PROTOCOL_VERSION_RESPONSE"),
        1 => Some("GET_READER_CAPABILITIES"),
        11 => Some("GET_READER_CAPABILITIES_RESPONSE"),
        20 => Some("ADD_ROSPEC"),
        30 => Some("ADD_ROSPEC_RESPONSE"),
        21 => Some("DELETE_ROSPEC"),
        31 => Some("DELETE_ROSPEC_RESPONSE"),
        22 => Some("START_ROSPEC"),
        32 => Some("START_ROSPEC_RESPONSE"),
        23 => Some("STOP_ROSPEC"),
        33 => Some("STOP_ROSPEC_RESPONSE"),
        24 => Some("ENABLE_ROSPEC"),
        34 => Some("ENABLE_ROSPEC_RESPONSE"),
        25 => Some("DISABLE_ROSPEC"),
        35 => Some("DISABLE_ROSPEC_RESPONSE"),
        26 => Some("GET_ROSPECS"),
        36 => Some("GET_ROSPECS_RESPONSE"),
        40 => Some("ADD_ACCESS_SPEC"),
        50 => Some("ADD_ACCESS_SPEC_RESPONSE"),
        41 => Some("DELETE_ACCESS_SPEC"),
        51 => Some("DELETE_ACCESS_SPEC_RESPONSE"),
        42 => Some("ENABLE_ACCESS_SPEC"),
        52 => Some("ENABLE_ACCESS_SPEC_RESPONSE"),
        43 => Some("DISABLE_ACCESS_SPEC"),
        53 => Some("DISABLE_ACCESS_SPEC_RESPONSE"),
        44 => Some("GET_ACCESS_SPECS"),
        54 => Some("GET_ACCESS_SPECS_RESPONSE"),
        45 => Some("CLIENT_REQUEST_OP"),
        55 => Some("CLIENT_REQUEST_OP_RESPONSE"),
        2 => Some("GET_READER_CONFIG"),
        12 => Some("GET_READER_CONFIG_RESPONSE"),
        3 => Some("SET_READER_CONFIG"),
        13 => Some("SET_READER_CONFIG_RESPONSE"),
        14 => Some("CLOSE_CONNECTION"),
        4 => Some("CLOSE_CONNECTION_RESPONSE"),
        60 => Some("GET_REPORT"),
        61 => Some("RO_ACCESS_REPORT"),
        62 => Some("KEEPALIVE"),
        72 => Some("KEEPALIVE_ACK"),
        63 => Some("READER_EVENT_NOTIFICATION"),
        64 => Some("ENABLE_EVENTS_AND_REPORTS"),
        100 => Some("ERROR_MESSAGE"),
        1023 => Some("CUSTOM_MESSAGE"),
        _ => None,
    }
}
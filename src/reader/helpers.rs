use super::{ANTENNA_STATUS_CONNECTED, ANTENNA_STATUS_DISCONNECTED};

pub fn antenna_status_str(status: u8) -> &'static str {
    if status == ANTENNA_STATUS_CONNECTED {
        return "-";
    }
    if status == ANTENNA_STATUS_DISCONNECTED {
        return "x";
    }
    return "";
}
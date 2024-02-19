use serde::{Serialize, Deserialize};

pub const READ_STATUS_UNUSED: u8 = 0;
pub const READ_STATUS_USED: u8 = 1;
pub const READ_STATUS_TOO_SOON: u8 = 2;

pub const READ_UPLOADED_FALSE: u8 = 0;
pub const READ_UPLOADED_TRUE: u8 = 1;

pub const READ_KIND_CHIP: &str = "reader";
pub const READ_KIND_MANUAL: &str = "manual";
pub const READ_IDENT_TYPE_CHIP: &str = "chip";
pub const READ_IDENT_TYPE_BIB: &str = "bib";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all="snake_case")]
pub struct Read {
    // ID should be implemented database side.
    #[serde(skip)]
    id: u64,
    // These fields should be received from the reader.
    identifier: String,
    seconds: u64,
    milliseconds: u32,
    reader_seconds: u64,
    reader_milliseconds: u32,
    antenna: u32,
    reader: String,
    rssi: String,
    // These fields are used for the remote API.
    // Should always be set to the same values.
    ident_type: String,
    #[serde(rename="type")]
    kind: String,
    // Status will be used for when the system processes reads.
    // do not serialize these fields
    #[serde(skip)]
    status: u8,
    #[serde(skip)]
    uploaded: u8,
}

impl Read {
    pub fn new(
        id: u64,
        chip: String,
        seconds: u64,
        milliseconds: u32,
        reader_seconds: u64,
        reader_milliseconds: u32,
        antenna: u32,
        reader: String,
        rssi: String,
        status: u8,
        uploaded: u8,
    ) -> Read{
            Read {
                id,
                identifier: chip,
                seconds,
                milliseconds,
                reader_seconds,
                reader_milliseconds,
                antenna,
                reader,
                rssi,
                status,
                uploaded,
                ident_type: String::from(READ_IDENT_TYPE_CHIP),
                kind: String::from(READ_KIND_CHIP)
            }
    }

    pub fn equals(&self, other: &Read) -> bool {
        self.identifier == other.identifier &&
        self.seconds == other.seconds &&
        self.milliseconds == other.milliseconds &&
        self.reader_seconds == other.reader_seconds &&
        self.reader_milliseconds == other.reader_milliseconds &&
        self.antenna == other.antenna &&
        self.reader == other.reader &&
        self.status == other.status &&
        self.uploaded == other.uploaded
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn chip(&self) -> &str {
        &self.identifier
    }

    pub fn seconds(&self) -> u64 {
        self.seconds
    }

    pub fn milliseconds(&self) -> u32 {
        self.milliseconds
    }

    pub fn reader_seconds(&self) -> u64 {
        self.reader_seconds
    }

    pub fn reader_milliseconds(&self) -> u32 {
        self.reader_milliseconds
    }

    pub fn antenna(&self) -> u32 {
        self.antenna
    }

    pub fn reader(&self) -> &str {
        &self.reader
    }

    pub fn rssi(&self) -> &str {
        &self.rssi
    }

    pub fn status(&self) -> u8 {
        self.status
    }

    pub fn uploaded(&self) -> u8 {
        self.uploaded
    }

    pub fn set_status(&mut self, status: u8) {
        self.status = status;
    }

    pub fn set_uploaded(&mut self, uploaded: u8) {
        self.uploaded = uploaded;
    }

    pub fn ident_type(&self) -> &str {
        &self.ident_type
    }

    pub fn is_valid(&self) -> bool {
        let mut output = true;
        match self.ident_type.as_str() {
            READ_IDENT_TYPE_CHIP | READ_IDENT_TYPE_BIB => {},
            _ => output = false,
        }
        match self.kind.as_str() {
            READ_KIND_MANUAL | READ_KIND_CHIP => {}
            _ => output = false,
        }
        output
    }
}
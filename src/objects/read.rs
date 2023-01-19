pub const READ_STATUS_UNUSED: u8 = 0;
pub const READ_STATUS_USED: u8 = 1;
pub const READ_STATUS_TOO_SOON: u8 = 3;

pub const READ_UPLOADED_FALSE: u8 = 0;

pub struct Read {
    // ID should be implemented database side.
    id: u64,
    // These fields should be received from the reader.
    chip: String,
    seconds: u64,
    milliseconds: u32,
    antenna: u32,
    reader: String,
    rssi: String,
    // Status will be used for when the system processes reads.
    status: u8,
    uploaded: u8,
}

impl Read {
    pub fn new(
        id: u64,
        chip: String,
        seconds: u64,
        milliseconds: u32,
        antenna: u32,
        reader: String,
        rssi: String,
        status: u8,
        uploaded: u8,
    ) -> Read{
            Read {
                id,
                chip,
                seconds,
                milliseconds,
                antenna,
                reader,
                rssi,
                status,
                uploaded
            }
    }

    pub fn equals(&self, other: &Read) -> bool {
        self.chip == other.chip &&
        self.seconds == other.seconds &&
        self.milliseconds == other.milliseconds &&
        self.antenna == other.antenna &&
        self.reader == other.reader &&
        self.status == other.status
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn chip(&self) -> &str {
        &self.chip
    }

    pub fn seconds(&self) -> u64 {
        self.seconds
    }

    pub fn milliseconds(&self) -> u32 {
        self.milliseconds
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
}
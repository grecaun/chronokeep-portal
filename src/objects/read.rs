pub struct Read {
    // ID should be implemented database side.
    pub id: u64,
    // These fields should be received from the reader.
    pub chip: String,
    pub seconds: u64,
    pub milliseconds: u32,
    pub antenna: u32,
    pub reader: String,
    pub rssi: String,
    // Status will be used for when the system processes reads.
    pub status: u16,
}

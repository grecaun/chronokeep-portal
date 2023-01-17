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
    status: u16,
}
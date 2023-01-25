pub fn get_supported_version() {
    todo!()
}

pub fn set_protocol_version() {
    todo!()
}

pub fn get_reader_capabilities() {
    todo!()
}

pub fn add_rospec() {
    todo!()
}

pub fn delete_rospec() {
    todo!()
}

pub fn start_rospec() {
    todo!()
}

pub fn stop_rospec() {
    todo!()
}

pub fn enable_rospec() {
    todo!()
}

pub fn disable_rospec() {
    todo!()
}

pub fn get_rospecs(id: &u32) -> [u8;10] {
    let header: u16 = (1 << 10) + 35;
    let length: u32 = 10;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // convert 32 bits to four 8 bit unsigned ints
        ((length & 0xFF000000) >> 24) as u8,
        ((length & 0x00FF0000) >> 16) as u8,
        ((length & 0x0000FF00) >> 8) as u8,
        (length & 0x000000FF) as u8,
        // convert another 32 bits to four 8 bit unsigned ints
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
    ]
}

pub fn add_access_spec() {
    todo!()
}

pub fn delete_access_spec() {
    todo!()
}

pub fn enable_access_spec() {
    todo!()
}

pub fn disable_accesss_spec() {
    todo!()
}

pub fn get_access_specs() {
    todo!()
}

pub fn client_request_op() {
    todo!()
}

pub fn get_reader_config() {
    todo!()
}

pub fn set_reader_config() {
    todo!()
}

pub fn close_connection(id: &u32) -> [u8;10] {
    let header: u16 = (1 << 10) + 35;
    let length: u32 = 10;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // convert 32 bits to four 8 bit unsigned ints
        ((length & 0xFF000000) >> 24) as u8,
        ((length & 0x00FF0000) >> 16) as u8,
        ((length & 0x0000FF00) >> 8) as u8,
        (length & 0x000000FF) as u8,
        // convert another 32 bits to four 8 bit unsigned ints
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
    ]
}

pub fn get_report() {
    todo!()
}

pub fn keepalive() {
    todo!()
}

pub fn keepalive_ack(id: &u32) -> [u8;10] {
    let header: u16 = (1 << 10) + 35;
    let length: u32 = 10;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // convert 32 bits to four 8 bit unsigned ints
        ((length & 0xFF000000) >> 24) as u8,
        ((length & 0x00FF0000) >> 16) as u8,
        ((length & 0x0000FF00) >> 8) as u8,
        (length & 0x000000FF) as u8,
        // convert another 32 bits to four 8 bit unsigned ints
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
    ]
}

pub fn reader_event_notification() {
    todo!()
}

pub fn enable_events_and_reports() {
    todo!()
}
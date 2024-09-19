use crate::llrp::{message_types, parameter_types};

pub fn get_reader_capabilities(id: &u32) -> [u8;24] {
    let header: u16 = (1 << 10) + message_types::GET_READER_CAPABILITIES;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // length of 24 (0x18)
        0x00, 0x00, 0x00, 0x18,
        // convert id from 32 bits to four bytes
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
        // all capabilities
        0x00,
        // custom parameter type 1023 -- this might be zebra specific
        0x03, 0xFF,
        // length 13
        0x00, 0x0D,
        // vendor ID 161
        0x00, 0x00, 0x00, 0xA1,
        // param type -- MotoGeneralRequestCapabilities
        0x00, 0x00, 0x00, 0x32,
        // RequestedData -- all
        0x00
    ]
}

pub fn add_rospec(id: &u32, rospec_id: &u32) -> [u8;96] {
    let header: u16 = (1 << 10) + message_types::ADD_ROSPEC;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // length 96
        0x00, 0x00, 0x00, 0x60,
        // convert id to 4 bytes
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
        // TLV Param - RO Spec
        ((parameter_types::RO_SPEC & 0xFF00) >> 8) as u8,
        (parameter_types::RO_SPEC & 0xFF) as u8,
        // length 86
        0x00, 0x56,
        // Rospec ID
        ((rospec_id & 0xFF000000) >> 24) as u8,
        ((rospec_id & 0xFF0000) >> 16) as u8,
        ((rospec_id & 0xFF00) >> 8) as u8,
        (rospec_id & 0xFF) as u8,
        // priority 0-7, lower is higher
        0x00,
        // Current state - 0 disabled, 1 enabled, 2 active
        0x00,
        // TLV Param - RO Bound Spec
        ((parameter_types::RO_BOUNDARY_SPEC & 0xFF00) >> 8) as u8,
        (parameter_types::RO_BOUNDARY_SPEC & 0xFF) as u8,
        // Length 18
        0x00, 0x12,
            // TLV Param - RO Spec Start Trigger
            ((parameter_types::RO_SPEC_START_TRIGGER & 0xFF00) >> 8) as u8,
            (parameter_types::RO_SPEC_START_TRIGGER & 0xFF) as u8,
            // Length 5
            0x00, 0x05,
            // trigger type - 0 null, starts with START_ROSPEC, 1 -immediate, 2 periodic, 3 GPI
            0x00,
            // TLV Param - RO Spec Stop Trigger
            ((parameter_types::RO_SPEC_STOP_TRIGGER & 0xFF00) >> 8) as u8,
            (parameter_types::RO_SPEC_STOP_TRIGGER & 0xFF) as u8,
            // Length 9
            0x00, 0x09,
            // trigger type - 0 null, 1 Duration, 2 GPI with timeout value
            0x00,
            // Duration trigger value - ignored when trigger type isn't 1
            0x00, 0x00, 0x00, 0x00,
        // TLV Param - AI Spec
        ((parameter_types::AI_SPEC & 0xFF00) >> 8) as u8,
        (parameter_types::AI_SPEC & 0xFF) as u8,
        // Length 24
        0x00, 0x18,
        // antennas - 1 - set to one and set id of 0 means all antennas
        0x00, 0x01,
        // antenna id
        0x00, 0x00,
            // TLV Param - AI Spec Stop
            ((parameter_types::AI_SPEC_STOP_TRIGGER & 0xFF00) >> 8) as u8,
            (parameter_types::AI_SPEC_STOP_TRIGGER & 0xFF) as u8,
            // Length 9
            0x00, 0x09,
            // trigger type 0 = null
            0x00,
            // duration
            0x00, 0x00, 0x00, 0x00,
            // TLV Param - Inventory Parameter Spec ID
            ((parameter_types::INVENTORY_PARAMETER_SPEC & 0xFF00) >> 8) as u8,
            (parameter_types::INVENTORY_PARAMETER_SPEC & 0xFF) as u8,
            // Length 7
            0x00, 0x07,
            // inventory parameter spec id - 19
            0x00, 0x13,
            // protocol id
            0x01,
        // TLV Param - RO Report Spec
        ((parameter_types::RO_REPORT_SPEC & 0xFF00) >> 8) as u8,
        (parameter_types::RO_REPORT_SPEC & 0xFF) as u8,
        // length 24
        0x00, 0x22,
        // ro report trigger - 2 -- this and N=1 tells it to report every read to us
        0x02,
        // n - 1
        0x00, 0x01,
            // TLV Param - Tag Report Content Selector
            ((parameter_types::TAG_REPORT_CONTENT_SELECTOR & 0xFF00) >> 8) as u8,
            (parameter_types::TAG_REPORT_CONTENT_SELECTOR & 0xFF) as u8,
            // length 11
            0x00, 0x0b,
            // 1... .... .... .... - enable rospec id - yes
            // .0.. .... .... .... - enable spec index - no
            // ..0. .... .... .... - enable inventory spec id - no
            // ...1 .... .... .... - enable antenna id - yes
            // .... 0... .... .... - enable channel index - no
            // .... .1.. .... .... - enable peak rssi - yes
            // .... ..1. .... .... - enable first seen timestamp - yes
            // .... ...0 .... .... - enable last seen timestamp - no
            // .... .... 0... .... - enable tag seen count - no
            // .... .... .0.. .... - enable accessspec id - no
            0x96, 0x00,
                // TLV Param - C1G2 EPC Memory Selector
                ((parameter_types::C1G2_EPC_MEMORY_SELECTOR & 0xFF00) >> 8) as u8,
                (parameter_types::C1G2_EPC_MEMORY_SELECTOR & 0xFF) as u8,
                // length 5
                0x00, 0x05,
                // 0... .... - enable crc - no
                // .0.. .... - enable pc bits - no
                // ..0. .... - enable xpc bits - no
                0x00,
            // Custom Parameter
            ((parameter_types::CUSTOM_PARAMETER & 0xFF00) >> 8) as u8,
            (parameter_types::CUSTOM_PARAMETER & 0xFF) as u8,
            // length 16
            0x00, 0x10,
            // vendor 161
            0x00, 0x00, 0x00, 0xA1,
            // Moto Tag Report Content Selector - 708
            0x00, 0x00, 0x02, 0xC4,
            // 0... .... enable zoneid in tag report - no
            // .0.. .... enable zonename in tag report - no
            // ..0. .... enable physical port in tag report - no
            // ...0 .... enable phase in tag report - no
            // .... 0... enable gps in tag report - no
            // .... .0.. enable mlt algorithm report - no
            0x00, 0x00,
            // reserved bytes
            0x00, 0x00
    ]
}

pub fn delete_rospec(id: &u32, rospec_id: &u32) -> [u8;14] {
    len_14(message_types::DELETE_ROSPEC, id, rospec_id)
}

pub fn start_rospec(id: &u32, rospec_id: &u32) -> [u8;14] {
    len_14(message_types::START_ROSPEC, id, rospec_id)
}

pub fn stop_rospec(id: &u32, rospec_id: &u32) -> [u8;14] {
    len_14(message_types::STOP_ROSPEC, id, rospec_id)
}

pub fn enable_rospec(id: &u32, rospec_id: &u32) -> [u8;14] {
    len_14(message_types::ENABLE_ROSPEC, id, rospec_id)
}

pub fn disable_rospec(id: &u32, rospec_id: &u32) -> [u8;14] {
    len_14(message_types::DISABLE_ROSPEC, id, rospec_id)
}

pub fn get_rospecs(id: &u32) -> [u8;10] {
    len_10(message_types::GET_ROSPECS, id)
}

pub fn delete_access_spec(id: &u32, as_id: &u32) -> [u8;14] {
    len_14(message_types::DELETE_ACCESS_SPEC, id, as_id)
}

pub fn purge_tags(id: &u32) -> [u8;16] {
    let header: u16 = (1 << 10) + message_types::CUSTOM_MESSAGE;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // length 16
        0x00, 0x00, 0x00, 0x10,
        // convert id to 4 bytes
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
        // vendor ID 161
        0x00, 0x00, 0x00, 0xA1,
        // other info
        0x03, 0x00
    ]
}

pub fn get_access_specs(id: &u32) -> [u8;10] {
    len_10(message_types::GET_ACCESS_SPECS, id)
}

pub fn get_reader_config(id: &u32, ant_id: &u16, config: &u8, gpi_port: &u16, gpo_port: &u16) -> [u8; 17] {
    let header: u16 = (1 << 10) + message_types::GET_READER_CONFIG;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // length 20
        0x00, 0x00, 0x00, 0x11,
        // convert id to 4 bytes
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0xFF0000) >> 16) as u8,
        ((id & 0xFF00) >> 8) as u8,
        (id & 0xFF) as u8,
        // antenna - 0 is all
        ((ant_id & 0xFF00) >> 8) as u8,
        (ant_id & 0xFF) as u8,
        // config value -
        //      0 all,
        //      1 identification,
        //      2 antenna properties,
        //      3 antenna configuration,
        //      4 ROReportSpec,
        //      5 ReaderEventNotificationSpec,
        //      6 AccessReportSpec,
        //      7 LLRPConfigurationStateValue,
        //      8 KeepaliveSpec,
        //      9 GPIPortCurrentState,
        //      10 GPOWriteData,
        //      11 EventsAndReports
        *config,
        // GPIPortNum
        ((gpi_port & 0xFF00) >> 8) as u8,
        (gpi_port & 0xFF) as u8,
        // GPOPortNum
        ((gpo_port & 0xFF00) >> 8) as u8,
        (gpo_port & 0xFF) as u8,
    ]
}

pub fn set_keepalive(id: &u32) -> [u8;20] {
    let header: u16 = (1 << 10) + message_types::SET_READER_CONFIG;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // length 20
        0x00, 0x00, 0x00, 0x14,
        // convert id to 4 bytes
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
        // Don't restore factory defaults
        0x00,
        // Keepalive spec
        ((parameter_types::KEEPALIVE_SPEC & 0xFF00) >> 8) as u8,
        (parameter_types::KEEPALIVE_SPEC & 0xFF) as u8,
        // length - 9
        0x00, 0x09,
        // keepalive trigger type - periodic
        0x01,
        // time interval - 2000 (2 seconds) (0x07 0xD0)
        0x00, 0x00, 0x07, 0xD0
    ]
}

pub fn set_no_filter(id: &u32) -> [u8;27] {
    let header: u16 = (1 << 10) + message_types::SET_READER_CONFIG;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // length 27
        0x00, 0x00, 0x00, 0x1B,
        // convert id to 4 bytes
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
        // Don't restore factory defaults
        0x00,
        // Custom Parameter
        0x03, 0xFF,
        // Length 16
        0x00, 0x10,
        // vendor ID 161
        0x00, 0x00, 0x00, 0xA1,
        // Subtype 255
        0x00, 0x00, 0x00, 0xFF,
        // F is the first bit of this byte, 0 means not enabled
        0x00,
        // Next three bytes are reserved
        0x00, 0x00, 0x00
    ]
}

pub fn set_reader_config(id: &u32) -> [u8;41] {
    let header: u16 = (1 << 10) + message_types::SET_READER_CONFIG;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // length 41
        0x00, 0x00, 0x00, 0x29,
        // convert id to 4 bytes
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
        // Don't restore factory defaults
        0x00,
        // Param -- Reader Event Notification Spec
        ((parameter_types::READER_EVENT_NOTIFICATION_SPEC & 0xFF00) >> 8) as u8,
        (parameter_types::READER_EVENT_NOTIFICATION_SPEC & 0xFF) as u8,
        // length 25
        0x00, 0x19,
        // Param -- Event Notification State
        ((parameter_types::EVENT_NOTIFICATION_STATE & 0xFF00) >> 8) as u8,
        (parameter_types::EVENT_NOTIFICATION_STATE & 0xFF) as u8,
        // length 7
        0x00, 0x07,
        // Event Type: ROSpec event - 2
        0x00, 0x02,
        // Notification state: Yes
        0x80,
        // Param -- Event Notification State
        ((parameter_types::EVENT_NOTIFICATION_STATE & 0xFF00) >> 8) as u8,
        (parameter_types::EVENT_NOTIFICATION_STATE & 0xFF) as u8,
        // length 7
        0x00, 0x07,
        // Event type: Report buffer fill warning - 3
        0x00, 0x03,
        // Notification state: Yes
        0x80,
        // Param -- Event Notification State
        ((parameter_types::EVENT_NOTIFICATION_STATE & 0xFF00) >> 8) as u8,
        (parameter_types::EVENT_NOTIFICATION_STATE & 0xFF) as u8,
        // length 7
        0x00, 0x07,
        // Event type: Reader exception event - 4
        0x00, 0x04,
        // Notification state: Yes
        0x80,
        // Param - Events and Reports
        ((parameter_types::EVENTS_AND_REPORTS & 0xFF00) >> 8) as u8,
        (parameter_types::EVENTS_AND_REPORTS & 0xFF) as u8,
        // length 7
        0x00, 0x05,
        // Hold events and reports upon reconnect: yes
        0x80

    ]
}

pub fn close_connection(id: &u32) -> [u8;10] {
    len_10(message_types::CLOSE_CONNECTION, id)
}

pub fn get_report() {
    todo!()
}

pub fn keepalive_ack(id: &u32) -> [u8;10] {
    len_10(message_types::KEEPALIVE_ACK, id)
}

pub fn enable_events_and_reports(id: &u32) -> [u8;10] {
    len_10(message_types::ENABLE_EVENTS_AND_REPORTS, id)
}

fn len_14(kind: u16, id: &u32, s_id: &u32) -> [u8;14] {
    let header: u16 = (1 << 10) + kind;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // length of 14 (0x0e)
        0x00, 0x00, 0x00, 0x0E,
        // convert id from 32 bits to four bytes
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
        // convert rospec id from 32 bits to four bytes
        ((s_id & 0xFF000000) >> 24) as u8,
        ((s_id & 0x00FF0000) >> 16) as u8,
        ((s_id & 0x0000FF00) >> 8) as u8,
        (s_id & 0x000000FF) as u8,
    ]
}

fn len_10(kind: u16, id: &u32) -> [u8;10] {
    let header: u16 = (1 << 10) + kind;
    [
        // convert 16 bits to two 8 bit unsigned ints
        ((header & 0xFF00) >> 8) as u8,
        (header & 0x00FF) as u8,
        // length of 10 (0x0a)
        0x00, 0x00, 0x00, 0x0A,
        // convert id from 32 bits to four bytes
        ((id & 0xFF000000) >> 24) as u8,
        ((id & 0x00FF0000) >> 16) as u8,
        ((id & 0x0000FF00) >> 8) as u8,
        (id & 0x000000FF) as u8,
    ]
}
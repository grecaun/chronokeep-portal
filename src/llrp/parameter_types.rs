// General Parameters
pub const UTC_TIMESTAMP: u16 = 128;
pub const UPTIME: u16 = 129;

// Reader Device Capabilities
pub const GENERAL_DEVICE_CAPABILITIES: u16 = 137;
pub const MAXIMUM_RECEIVE_SENSITIVITY: u16 = 363;
pub const RECEIVE_SENSITIVITY_TABLE_ENTRY: u16 = 139;
pub const PER_ANTENNA_RECEIVE_SENSITIVITY_RANGE: u16 = 149;
pub const PER_ANTENNA_AIR_PROTOCOL: u16 = 140;
pub const GPIO_CAPABILITIES: u16 = 141;
pub const LLRP_CAPABILITIES: u16 = 142;
pub const REGULATORY_CAPABILITIES: u16 = 143;
pub const UHF_BAND_CAPABILITIES: u16 = 144;
pub const TRANSMIT_POWER_LEVEL_TABLE_ENTRY: u16 = 145;
pub const FREQUENCY_INFORMATION: u16 = 146;
pub const FREQUENCY_HOP_TABLE: u16 = 147;
pub const FIXED_FREQUENCY_TABLE: u16 = 148;
pub const RF_SURVEY_FREQUENCY_CAPABILITIES: u16 = 365;

// Reader Operation Parameters
pub const RO_SPEC: u16 = 177;
pub const RO_BOUNDARY_SPEC: u16 = 178;
pub const RO_SPEC_START_TRIGGER: u16 = 179;
pub const PERIODIC_TRIGGER_VALUE: u16 = 180;
pub const GPI_TRIGGER_VALUE: u16 = 181;
pub const RO_SPEC_STOP_TRIGGER: u16 = 182;
pub const AI_SPEC: u16 = 183;
pub const AI_SPEC_STOP_TRIGGER: u16 = 184;
pub const TAG_OBSERVATION_TRIGGER: u16 = 185;
pub const INVENTORY_PARAMETER_SPEC: u16 = 186;
pub const RF_SURVEY_SPEC: u16 = 187;
pub const RF_SURVEY_SPEC_STOP_TRIGGER: u16 = 188;
pub const LOOP_SPEC: u16 = 355;

// Access Operation Parameters
pub const ACCESS_SPEC: u16 = 207;
pub const ACCESS_SPEC_STOP_TRIGGER: u16 = 208;
pub const ACCESS_COMMAND: u16 = 209;
pub const CLIENT_REQUEST_OP_SPEC: u16 = 210;
pub const CLIENT_REQUEST_RESPONSE: u16 = 211;

// Configuration Parameters
pub const LLRP_CONFIGURATION_STATE_VALUE: u16 = 217;
pub const IDENTIFICATION: u16 = 218;
pub const GPO_WRITE_DATA: u16 = 219;
pub const KEEPALIVE_SPEC: u16 = 220;
pub const ANTENNA_PROPERTIES: u16 = 221;
pub const ANTENNA_CONFIGURATION: u16 = 222;
pub const RF_RECEIVER: u16 = 223;
pub const RF_TRANSMITTER: u16 = 224;
pub const GPI_PORT_CURRENT_STATE: u16 = 225;
pub const EVENTS_AND_REPORTS: u16 = 226;

// Reporting Parameters
pub const RO_REPORT_SPEC: u16 = 237;
pub const TAG_REPORT_CONTENT_SELECTOR: u16 = 238;
pub const ACCESS_REPORT_SPEC: u16 = 239;
pub const TAG_REPORT_DATA: u16 = 240;
pub const EPC_DATA: u16 = 241;

// TV Encodings (first bit 1, bits 2-8 are type)
pub const EPC_96: u16 = 13;
pub const RO_SPEC_ID: u16 = 9;
pub const SPEC_INDEX: u16 = 14;
pub const INVENTORY_PARAMETER_SPEC_ID: u16 = 10;
pub const ANTENNA_ID: u16 = 1;
pub const PEAK_RSSI: u16 = 6;
pub const CHANNEL_INDEX: u16 = 7;
pub const FIRST_SEEN_TIMESTAMP_UTC: u16 = 2;
pub const FIRST_SEEN_TIMESTAMP_UPTIME: u16 = 3;
pub const LAST_SEEN_TIMESTAMP_UTC: u16 = 4;
pub const LAST_SEEN_TIMESTAMP_UPTIME: u16 = 5;
pub const TAG_SEEN_COUNT: u16 = 8;
pub const CLIENT_REQUEST_OP_SPEC_RESULT: u16 = 15;
pub const ACCESS_SPEC_ID: u16 = 16;

// Non-TV
pub const RF_SURVEY_REPORT_DATA: u16 = 242;
pub const FREQUENCY_RSSI_LEVEL_ENTRY: u16 = 243;
pub const READER_EVENT_NOTIFICATION_SPEC: u16 = 244;
pub const EVENT_NOTIFICATION_STATE: u16 = 245;
pub const READER_EVENT_NOTIFICATION_DATA: u16 = 246;
pub const HOPPING_EVENT: u16 = 247;
pub const GPI_EVENT: u16 = 248;
pub const RO_SPEC_EVENT: u16 = 249;
pub const REPORT_BUFFER_LEVEL_WARNING_EVENT: u16 = 250;
pub const REPORT_BUFFER_OVERFLOW_ERROR_EVENT: u16 = 251;
pub const READER_EXCEPTION_EVENT: u16 = 252;
pub const RF_SURVEY_EVENT: u16 = 253;
pub const AI_SPEC_EVENT: u16 = 254;
pub const ANTENNA_EVENT: u16 = 255;
pub const CONNECTION_ATTEMPT_EVENT: u16 = 256;
pub const CONNECTION_CLOSE_EVENT: u16 = 257;
pub const SPEC_LOOP_EVENT: u16 = 356;

// LLRP Error Parameters
pub const LLRP_STATUS: u16 = 287;
pub const FIELD_ERROR: u16 = 288;
pub const PARAMETER_EVENT: u16 = 289;
pub const CUSTOM_PARAMETER: u16 = 1023;

// Air Protocol Specific Parameters

// Class-1 Generation-2 (C1G2) Protocol Parameters
pub const C1G2_LLRP_CAPABILITIES: u16 = 327;
pub const C1G2_UHF_MODE_TABLE: u16 = 328;
pub const C1G2_UHF_MODE_TABLE_ENTRY: u16 = 329;

// Reader Operations Parameters
pub const C1G2_INVENTORY_COMMAND: u16 = 330;
pub const C1G2_FILTER: u16 = 331;
pub const C1G2_TAG_INVENTORY_MAST: u16 = 332;
pub const C1G2_TAG_INVENTORY_STATE_AWARE_FILTER_ACTION: u16 = 333;
pub const C1G2_TAG_INVENTORY_STATE_UNAWARE_FILTER_ACTION: u16 = 334;
pub const C1G2_RF_CONTROL: u16 = 335;
pub const C1G2_SINGULATION_CONTROL: u16 = 336;
pub const C1G2_TAG_INVENTORY_STATE_AWARE_SINGULATION_ACTION: u16 = 337;

// Access Operation Parameters
pub const C1G2_TAG_SPEC: u16 = 338;
pub const C1G2_TARGET_TAG: u16 = 339;
pub const C1G2_READ: u16 = 341;
pub const C1G2_WRITE: u16 = 342;
pub const C1G2_KILL: u16 = 343;
pub const C1G2_RECOMMISSION: u16 = 357;
pub const C1G2_LOCK: u16 = 344;
pub const C1G2_LOCK_PAYLOAD: u16 = 345;
pub const C1G2_BLOCK_ERASE: u16 = 346;
pub const C1G2_BLOCK_WRITE: u16 = 347;
pub const C1G2_BLOCK_PERMALOCK: u16 = 358;
pub const C1G2_GET_BLOCK_PERMALOCK_STATUS: u16 = 359;

// Reporting Parameters
pub const C1G2_EPC_MEMORY_SELECTOR: u16 = 348;

// TV-Encoding (First bit 1, second through 8th are type)
pub const C1G2_PC: u16 = 12;
pub const C1G2_XPCW1: u16 = 19;
pub const C1G2_XPCW2: u16 = 20;
pub const C1G2_CRC: u16 = 11;
pub const C1G2_SINGULATION_DETAILS: u16 = 18;

// C1G2 OpSpec Results
pub const C1G2_READ_OP_SPEC_RESULT: u16 = 349;
pub const C1G2_WRITE_OP_SPEC_RESULT: u16 = 350;
pub const C1G2_KILL_OP_SPEC_RESULT: u16 = 351;
pub const C1G2_RECOMMISSION_OP_SPEC_RESULT: u16 = 360;
pub const C1G2_LOCK_OP_SPEC_RESULT: u16 = 352;
pub const C1G2_BLOCK_ERASE_OP_SPEC_RESULT: u16 = 353;
pub const C1G2_BLOCK_WRITE_OP_SPEC_RESULT: u16 = 354;
pub const C1G2_BLOCK_PERMALOCK_OP_SPEC_RESULT: u16 = 361;
pub const C1G2_GET_BLOCK_PERMALOCK_STATUS_OP_SPEC_RESULT: u16 = 362;

// LLRP Status Codes
pub const M_SUCCESS: u16 = 0;
pub const M_PARAMETER_ERROR: u16 = 100;
pub const M_FIELD_ERROR: u16 = 101;
pub const M_UNEXPECTED_PARAMETER: u16 = 102;
pub const M_MISSING_PARAMETER: u16 = 103;
pub const M_DUPLICATE_PARAMETER: u16 = 104;
pub const M_OVERFLOW_PARAMETER: u16 = 105;
pub const M_OVERFLOW_FIELD: u16 = 106;
pub const M_UNKNOWN_PARAMETER: u16 = 107;
pub const M_UNKNOWN_FIELD: u16 = 108;
pub const M_UNSUPPORTED_MESSAGE: u16 = 109;
pub const M_UNSUPPORTED_VERSION: u16 = 110;
pub const M_UNSUPPORTED_PARAMETER: u16 = 111;
pub const M_UNEXPECTED_MESSAGE: u16 = 112;
pub const P_PARAMETER_ERROR: u16 = 200;
pub const P_FIELD_ERROR: u16 = 201;
pub const P_UNEXPECTED_PARAMETER: u16 = 202;
pub const P_MISSING_PARAMETER: u16 = 203;
pub const P_DUPLICATE_PARAMETER: u16 = 204;
pub const P_OVERFLOW_PARAMETER: u16 = 205;
pub const P_OVERFLOW_FIELD: u16 = 206;
pub const P_UNKNOWN_PARAMETER: u16 = 207;
pub const P_UNKNOWN_FIELD: u16 = 208;
pub const P_UNSUPPORTED_PARAMETER: u16 = 209;
pub const A_INVALID: u16 = 300;
pub const A_OUT_OF_RANGE: u16 = 301;
pub const R_DEVICE_ERROR: u16 = 401;

pub fn get_parameter_name(kind: u16) -> Option<&'static str> {
    match kind {
        128 => Some("UTC_TIMESTAMP"),
        129 => Some("UPTIME"),
        137 => Some("GENERAL_DEVICE_CAPABILITIES"),
        363 => Some("MAXIMUM_RECEIVE_SENSITIVITY"),
        139 => Some("RECEIVE_SENSITIVITY_TABLE_ENTRY"),
        149 => Some("PER_ANTENNA_RECEIVE_SENSITIVITY_RANGE"),
        140 => Some("PER_ANTENNA_AIR_PROTOCOL"),
        141 => Some("GPIO_CAPABILITIES"),
        142 => Some("LLRP_CAPABILITIES"),
        143 => Some("REGULATORY_CAPABILITIES"),
        144 => Some("UHF_BAND_CAPABILITIES"),
        145 => Some("TRANSMIT_POWER_LEVEL_TABLE_ENTRY"),
        146 => Some("FREQUENCY_INFORMATION"),
        147 => Some("FREQUENCY_HOP_TABLE"),
        148 => Some("FIXED_FREQUENCY_TABLE"),
        365 => Some("RF_SURVEY_FREQUENCY_CAPABILITIES"),
        177 => Some("RO_SPEC"),
        178 => Some("RO_BOUNDARY_SPEC"),
        179 => Some("RO_SPEC_START_TRIGGER"),
        180 => Some("PERIODIC_TRIGGER_VALUE"),
        181 => Some("GPI_TRIGGER_VALUE"),
        182 => Some("RO_SPEC_STOP_TRIGGER"),
        183 => Some("AI_SPEC"),
        184 => Some("AI_SPEC_STOP_TRIGGER"),
        185 => Some("TAG_OBSERVATION_TRIGGER"),
        186 => Some("INVENTORY_PARAMETER_SPEC"),
        187 => Some("RF_SURVEY_SPEC"),
        188 => Some("RF_SURVEY_SPEC_STOP_TRIGGER"),
        355 => Some("LOOP_SPEC"),
        207 => Some("ACCESS_SPEC"),
        208 => Some("ACCESS_SPEC_STOP_TRIGGER"),
        209 => Some("ACCESS_COMMAND"),
        210 => Some("CLIENT_REQUEST_OP_SPEC"),
        211 => Some("CLIENT_REQUEST_RESPONSE"),
        217 => Some("LLRP_CONFIGURATION_STATE_VALUE"),
        218 => Some("IDENTIFICATION"),
        219 => Some("GPO_WRITE_DATA"),
        220 => Some("KEEPALIVE_SPEC"),
        221 => Some("ANTENNA_PROPERTIES"),
        222 => Some("ANTENNA_CONFIGURATION"),
        223 => Some("RF_RECEIVER"),
        224 => Some("RF_TRANSMITTER"),
        225 => Some("GPI_PORT_CURRENT_STATE"),
        226 => Some("EVENTS_AND_REPORTS"),
        237 => Some("RO_REPORT_SPEC"),
        238 => Some("TAG_REPORT_CONTENT_SELECTOR"),
        239 => Some("ACCESS_REPORT_SPEC"),
        240 => Some("TAG_REPORT_DATA"),
        241 => Some("EPC_DATA"),
        13 => Some("EPC_96"),
        9 => Some("RO_SPEC_ID"),
        14 => Some("SPEC_INDEX"),
        10 => Some("INVENTORY_PARAMETER_SPEC_ID"),
        1 => Some("ANTENNA_ID"),
        6 => Some("PEAK_RSSI"),
        7 => Some("CHANNEL_INDEX"),
        2 => Some("FIRST_SEEN_TIMESTAMP_UTC"),
        3 => Some("FIRST_SEEN_TIMESTAMP_UPTIME"),
        4 => Some("LAST_SEEN_TIMESTAMP_UTC"),
        5 => Some("LAST_SEEN_TIMESTAMP_UPTIME"),
        8 => Some("TAG_SEEN_COUNT"),
        15 => Some("CLIENT_REQUEST_OP_SPEC_RESULT"),
        16 => Some("ACCESS_SPEC_ID"),
        242 => Some("RF_SURVEY_REPORT_DATA"),
        243 => Some("FREQUENCY_RSSI_LEVEL_ENTRY"),
        244 => Some("READER_EVENT_NOTIFICATION_SPEC"),
        245 => Some("EVENT_NOTIFICATION_STATE"),
        246 => Some("READER_EVENT_NOTIFICATION_DATA"),
        247 => Some("HOPPING_EVENT"),
        248 => Some("GPI_EVENT"),
        249 => Some("RO_SPEC_EVENT"),
        250 => Some("REPORT_BUFFER_LEVEL_WARNING_EVENT"),
        251 => Some("REPORT_BUFFER_OVERFLOW_ERROR_EVENT"),
        252 => Some("READER_EXCEPTION_EVENT"),
        253 => Some("RF_SURVEY_EVENT"),
        254 => Some("AI_SPEC_EVENT"),
        255 => Some("ANTENNA_EVENT"),
        256 => Some("CONNECTION_ATTEMPT_EVENT"),
        257 => Some("CONNECTION_CLOSE_EVENT"),
        356 => Some("SPEC_LOOP_EVENT"),
        287 => Some("LLRP_STATUS"),
        288 => Some("FIELD_ERROR"),
        289 => Some("PARAMETER_EVENT"),
        1023 => Some("CUSTOM_PARAMETER"),
        327 => Some("C1G2_LLRP_CAPABILITIES"),
        328 => Some("C1G2_UHF_MODE_TABLE"),
        329 => Some("C1G2_UHF_MODE_TABLE_ENTRY"),
        330 => Some("C1G2_INVENTORY_COMMAND"),
        331 => Some("C1G2_FILTER"),
        332 => Some("C1G2_TAG_INVENTORY_MAST"),
        333 => Some("C1G2_TAG_INVENTORY_STATE_AWARE_FILTER_ACTION"),
        334 => Some("C1G2_TAG_INVENTORY_STATE_UNAWARE_FILTER_ACTION"),
        335 => Some("C1G2_RF_CONTROL"),
        336 => Some("C1G2_SINGULATION_CONTROL"),
        337 => Some("C1G2_TAG_INVENTORY_STATE_AWARE_SINGULATION_ACTION"),
        338 => Some("C1G2_TAG_SPEC"),
        339 => Some("C1G2_TARGET_TAG"),
        341 => Some("C1G2_READ"),
        342 => Some("C1G2_WRITE"),
        343 => Some("C1G2_KILL"),
        357 => Some("C1G2_RECOMMISSION"),
        344 => Some("C1G2_LOCK"),
        345 => Some("C1G2_LOCK_PAYLOAD"),
        346 => Some("C1G2_BLOCK_ERASE"),
        347 => Some("C1G2_BLOCK_WRITE"),
        358 => Some("C1G2_BLOCK_PERMALOCK"),
        359 => Some("C1G2_GET_BLOCK_PERMALOCK_STATUS"),
        348 => Some("C1G2_EPC_MEMORY_SELECTOR"),
        12 => Some("C1G2_PC"),
        19 => Some("C1G2_XPCW1"),
        20 => Some("C1G2_XPCW2"),
        11 => Some("C1G2_CRC"),
        18 => Some("C1G2_SINGULATION_DETAILS"),
        349 => Some("C1G2_READ_OP_SPEC_RESULT"),
        350 => Some("C1G2_WRITE_OP_SPEC_RESULT"),
        351 => Some("C1G2_KILL_OP_SPEC_RESULT"),
        360 => Some("C1G2_RECOMMISSION_OP_SPEC_RESULT"),
        352 => Some("C1G2_LOCK_OP_SPEC_RESULT"),
        353 => Some("C1G2_BLOCK_ERASE_OP_SPEC_RESULT"),
        354 => Some("C1G2_BLOCK_WRITE_OP_SPEC_RESULT"),
        361 => Some("C1G2_BLOCK_PERMALOCK_OP_SPEC_RESULT"),
        _ => None,
    }
}

pub fn get_llrp_status_name(kind: u16) -> Option<&'static str> {
    match kind {
        0 => Some("M_SUCCESS"),
        100 => Some("M_PARAMETER_ERROR"),
        101 => Some("M_FIELD_ERROR"),
        102 => Some("M_UNEXPECTED_PARAMETER"),
        103 => Some("M_MISSING_PARAMETER"),
        104 => Some("M_DUPLICATE_PARAMETER"),
        105 => Some("M_OVERFLOW_PARAMETER"),
        106 => Some("M_OVERFLOW_FIELD"),
        107 => Some("M_UNKNOWN_PARAMETER"),
        108 => Some("M_UNKNOWN_FIELD"),
        109 => Some("M_UNSUPPORTED_MESSAGE"),
        110 => Some("M_UNSUPPORTED_VERSION"),
        111 => Some("M_UNSUPPORTED_PARAMETER"),
        112 => Some("M_UNEXPECTED_MESSAGE"),
        200 => Some("P_PARAMETER_ERROR"),
        201 => Some("P_FIELD_ERROR"),
        202 => Some("P_UNEXPECTED_PARAMETER"),
        203 => Some("P_MISSING_PARAMETER"),
        204 => Some("P_DUPLICATE_PARAMETER"),
        205 => Some("P_OVERFLOW_PARAMETER"),
        206 => Some("P_OVERFLOW_FIELD"),
        207 => Some("P_UNKNOWN_PARAMETER"),
        208 => Some("P_UNKNOWN_FIELD"),
        209 => Some("P_UNSUPPORTED_PARAMETER"),
        300 => Some("A_INVALID"),
        301 => Some("A_OUT_OF_RANGE"),
        401 => Some("R_DEVICE_ERROR"),
        _ => None
    }
}
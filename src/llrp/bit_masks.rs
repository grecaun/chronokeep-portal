// First 16 bits of our message are as follows.
// 3 bits reserved (should be 0)
// 3 bits llrp version (1 for 1.0, 2 for 1.1)
// 10 bits for Message Type
pub const RESERVED: u16 = 0xE000; // 1110 0000 0000 0000
pub const VERSION:  u16 = 0x1C00; // 0001 1100 0000 0000
pub const MSG_TYPE: u16 = 0x03FF; // 0000 0011 1111 1111

pub struct MsgTypeInfo {
    pub version: u16,
    pub kind: u16,
}

pub fn get_msg_type(bits: &u16) -> Result<MsgTypeInfo, &'static str> {
    if (bits & RESERVED) != 0 {
        return Err("invalid reserved field")
    }
    let vers = ( bits & VERSION ) >> 10;
    if vers != 1 && vers != 2 {
        return Err("invalid version specified")
    }
    Ok(MsgTypeInfo {
        version: vers,
        kind: bits & MSG_TYPE,
    })
}

// TV parameter masks
pub const TV_RESERVED:      u16 = 0x8000; // 1000 0000 0000 0000
pub const TV_TYPE:          u16 = 0x7F00; // 0111 1111 0000 0000

// Non-TV parameter masks
pub const PARAM_RESERVED:   u16 = 0xFA00; // 1111 1100 0000 0000
pub const PARAM_TYPE:       u16 = 0x03FF; // 0000 0011 1111 1111

pub struct ParamTypeInfo {
    pub tv: bool,
    pub kind: u16,
}

pub fn get_param_type(bits: &u16) -> Result<ParamTypeInfo, &'static str> {
    if (bits & TV_RESERVED) != 0 {
        return Ok(ParamTypeInfo {
            tv: true,
            kind: ( bits & TV_TYPE ) >> 8
        })
    }
    if ( bits & PARAM_RESERVED ) != 0 {
        return Err("invalid reserved field")
    }
    Ok(ParamTypeInfo {
        tv: false,
        kind: bits & PARAM_TYPE
    })
}
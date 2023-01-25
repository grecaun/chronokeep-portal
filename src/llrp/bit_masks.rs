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
    pub length: u32,
    pub id: u32,
}

pub fn get_msg_type(buf: &[u8]) -> Result<MsgTypeInfo, &'static str> {
    let bits: u16 = (u16::from(buf[0]) << 8) + u16::from(buf[1]);
    let mut length: u32 = 0;
    for i in 0..4 {
        length = (length << 8) + u32::from(buf[2+i])
    }
    let mut id: u32 = 0;
    for i in 0..4 {
        id = (id << 8) + u32::from(buf[6+i])
    }
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
        length,
        id,
    })
}

// TV parameter masks
pub const TV_RESERVED:      u16 = 0x8000; // 1000 0000 0000 0000
pub const TV_TYPE:          u16 = 0x7F00; // 0111 1111 0000 0000

// TLV parameter masks
pub const PARAM_RESERVED:   u16 = 0xFA00; // 1111 1100 0000 0000
pub const PARAM_TYPE:       u16 = 0x03FF; // 0000 0011 1111 1111
pub const PARAM_LENGTH:     u32 = 0xFFFF; // 1111 1111 1111 1111

pub struct ParamTypeInfo {
    pub tv: bool,
    pub kind: u16,
    pub length: u16,
}

pub fn get_param_type(bits: &u32) -> Result<ParamTypeInfo, &'static str> {
    let head: u16 = (bits >> 16) as u16;
    if (head & TV_RESERVED) != 0 {
        return Ok(ParamTypeInfo {
            tv: true,
            kind: ( head & TV_TYPE ) >> 8,
            length: 0,
        })
    }
    if ( head & PARAM_RESERVED ) != 0 {
        return Err("invalid reserved field")
    }
    Ok(ParamTypeInfo {
        tv: false,
        kind: head & PARAM_TYPE,
        length: (bits & PARAM_LENGTH) as u16,
    })
}
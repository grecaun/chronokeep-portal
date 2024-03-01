use crate::{sound_board::Voice, types};

pub const DEFAULT_SIGHTING_PERIOD: u32 = 5 * 60;
pub const DEFAULT_CHIP_TYPE: &str = types::TYPE_CHIP_DEC;
pub const DEFAULT_READ_WINDOW: u8 = 20;
pub const DEFAULT_PLAY_SOUND: bool = true;
pub const DEFAULT_VOLUME: f32 = 1.0;
pub const DEFAULT_VOICE: Voice = Voice::Emily;
pub const DEFAULT_AUTO_REMOTE: bool = false;
pub const DEFAULT_UPLOAD_INTERVAL: u64 = 10;
/*
Chronokeep Desktop - Race Scoring Software
Copyright (C) 2026 James Sentinella

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use crate::{sound_board::Voice, types};

pub const DEFAULT_CHIP_TYPE: &str = types::TYPE_CHIP_DEC;
pub const DEFAULT_READ_WINDOW: u8 = 20;
pub const DEFAULT_PLAY_SOUND: bool = true;
pub const DEFAULT_VOLUME: f32 = 1.0;
pub const DEFAULT_VOICE: Voice = Voice::Emily;
pub const DEFAULT_AUTO_REMOTE: bool = false;
pub const DEFAULT_UPLOAD_INTERVAL: u64 = 5;
pub const DEFAULT_ENABLE_NTFY: bool = false;
pub const DEFAULT_SCREEN_TYPE: &str = types::TYPE_SCREEN_ADAFRUIT;
pub const DEFAULT_BEEP_IGNORE: u8 = 60;

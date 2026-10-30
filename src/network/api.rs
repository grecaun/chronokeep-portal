/*
Chronokeep Portal - RFID Tag Reading Software
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

use serde::{Serialize, Deserialize};

pub const API_TYPE_CHRONOKEEP_REMOTE: &str = "CHRONOKEEP_REMOTE";
pub const API_TYPE_CHRONOKEEP_REMOTE_SELF: &str = "CHRONOKEEP_REMOTE_SELF";

pub const API_URI_CHRONOKEEP_REMOTE: &str = "https://remote.chronokeep.com/";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all="snake_case")]
pub struct Api {
    id: i64,
    nickname: String,
    kind: String,
    token: String,
    uri: String,
    enabled: Option<bool>,
    heartbeat: Option<bool>,
    device_name: Option<String>,
}

impl Api {
    pub fn new(
        id: i64,
        nickname: String,
        kind: String,
        token: String,
        uri: String,
        enabled: bool,
        heartbeat: bool,
        device_name: String) -> Api {
        Api {
            id,
            nickname,
            kind,
            token,
            uri,
            enabled: Some(enabled),
            heartbeat: Some(heartbeat),
            device_name: Some(device_name),
        }
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn nickname(&self) -> &str {
        &self.nickname
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn enabled(&self) -> bool {
        if let Some(enabled) = self.enabled {
            enabled
        } else {
            true // Default behavior is to treat an API as enabled if the value is not set
        }
    }

    pub fn heartbeat(&self) -> bool {
        if let Some(heartbeat) = self.heartbeat {
            heartbeat
        } else {
            false // Default behavior is to treat an API as not having heartbeat enabled if the value is not set
        }
    }

    pub fn device_name(&self) -> &str {
        self.device_name.as_deref().unwrap_or("")
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = Some(enabled);
    }

    pub fn equal(&self, other: &Api) -> bool {
        self.nickname == other.nickname &&
            self.kind == other.kind &&
            self.token == other.token &&
            self.uri == other.uri
    } 
}

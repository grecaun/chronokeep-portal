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

use serde::{Serialize, Deserialize};

pub const API_TYPE_CHRONOKEEP_REMOTE: &str = "CHRONOKEEP_REMOTE";
pub const API_TYPE_CHRONOKEEP_REMOTE_SELF: &str = "CHRONOKEEP_REMOTE_SELF";

pub const API_URI_CHRONOKEEP_REMOTE: &str = "https://remote.chronokeep.com/";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all="camelCase")]
pub struct Api {
    id: i64,
    nickname: String,
    kind: String,
    token: String,
    uri: String,
}

impl Api {
    pub fn new(
        id: i64,
        nickname: String,
        kind: String,
        token: String,
        uri: String) -> Api {
        Api {
            id,
            nickname,
            kind,
            token,
            uri,
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

    pub fn equal(&self, other: &Api) -> bool {
        self.nickname == other.nickname &&
            self.kind == other.kind &&
            self.token == other.token &&
            self.uri == other.uri
    } 
}

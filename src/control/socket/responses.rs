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

use serde::Serialize;

use crate::{network::api, objects::{read, setting}, reader::MAX_ANTENNAS, remote::uploader};

use super::{errors, notifications};

#[derive(Serialize, Debug)]
#[serde(tag="command", rename_all="snake_case")]
pub enum Responses {
    Readers {
        readers: Vec<Reader>,
    },
    ReaderAntennas{
        reader_name: String,
        antennas: [u8;MAX_ANTENNAS],
    },
    Error {
        error: errors::Errors,
    },
    Notification {
        kind: notifications::APINotification,
        time: String,
    },
    Settings {
        settings: Vec<setting::Setting>,
    },
    SettingsAll {
        settings: Vec<setting::Setting>,
        readers: Vec<Reader>,
        apis: Vec<api::Api>,
        auto_upload: uploader::Status,
        portal_version: &'static str,
    },
    ApiList {
        apis: Vec<api::Api>,
    },
    Reads {
        list: Vec<read::Read>,
    },
    Success {
        count: usize,
    },
    Time {
        local: String,
        utc: String,
    },
    ReadAutoUpload {
        status: uploader::Status,
    },
    ConnectionSuccessful {
        name: String,
        kind: String,
        version: usize,
        reads_subscribed: bool,
        readers: Vec<Reader>,
        updatable: bool,
        auto_upload: uploader::Status,
        portal_version: &'static str,
    },
    Keepalive,
    Disconnect,
}

#[derive(Serialize, Debug)]
#[serde(rename_all="snake_case")]
pub struct Reader {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub ip_address: String,
    pub port: u16,
    pub auto_connect: bool,
    pub reading: Option<bool>,
    pub connected: Option<bool>,
    pub antennas: [u8;MAX_ANTENNAS],
}

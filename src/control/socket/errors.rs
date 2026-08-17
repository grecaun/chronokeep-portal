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

#[derive(Serialize, Debug)]
#[serde(tag="error_type", rename_all="SCREAMING_SNAKE_CASE")]
pub enum Errors {
    UnknownCommand,
    TooManyConnections,
    TooManyRemoteApi,
    ServerError{
        message: String,
    },
    DatabaseError{
        message: String,
    },
    InvalidReaderType {
        message: String,
    },
    ReaderConnection {
        message: String,
    },
    NotFound,
    InvalidSetting {
        message: String,
    },
    InvalidApiType {
        message: String,
    },
    AlreadySubscribed {
        message: String,
    },
    AlreadyRunning,
    NotRunning,
    NoRemoteApi,
    StartingUp,
    InvalidRead,
    NotAllowed {
        message: String,
    },
}

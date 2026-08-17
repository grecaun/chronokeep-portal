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

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all="SCREAMING_SNAKE_CASE")]
pub enum APINotification {
    UpsDisconnected,
    UpsConnected,
    UpsOnBattery,
    UpsLowBattery,
    UpsOnline,
    ShuttingDown,
    Restarting,
    HighTemp,
    MaxTemp,
    BatteryLow,
    BatteryCritical,
}

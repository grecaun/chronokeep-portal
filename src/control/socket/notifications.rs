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
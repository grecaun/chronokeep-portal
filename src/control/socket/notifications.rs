use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag="notification_type", rename_all="SCREAMING_SNAKE_CASE")]
pub enum Notification {
    UpsDisconnected,
    UpsConnected,
    UpsOnBattery,
    UpsLowBattery,
    UpsOnline,
    ShuttingDown,
    Restarting,
    HighTemp,
    MaxTemp,
}
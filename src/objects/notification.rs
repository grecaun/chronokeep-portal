use serde::{Deserialize, Serialize};

use crate::control::socket::notifications::Notification;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RemoteNotification {
    #[serde(alias = "type")]
    pub kind: Notification,
    pub when: String
}
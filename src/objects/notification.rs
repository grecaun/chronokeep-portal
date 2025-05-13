use serde::{Deserialize, Serialize};

use crate::control::socket::notifications::APINotification;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RemoteNotification {
    #[serde(alias="type", rename="type")]
    pub kind: APINotification,
    pub when: String
}
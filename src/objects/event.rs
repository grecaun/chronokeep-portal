use serde::{Serialize, Deserialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Event {
    pub name: String,
    pub slug: String,
    pub website: String,
    pub image: String,
    pub contact_email: String,
    pub access_restricted: bool,
    #[serde(rename="type")]
    pub kind: String,
    pub recent_time: String,
}
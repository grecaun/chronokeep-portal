use serde::{Serialize, Deserialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Event {
    name: String,
    slug: String,
    website: String,
    image: String,
    contact_email: String,
    access_restricted: bool,
    #[serde(rename="type")]
    kind: String,
    recent_time: String,
}
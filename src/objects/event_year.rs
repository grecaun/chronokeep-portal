use serde::{Serialize, Deserialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct EventYear {
    pub year: String,
    pub date_time: String,
    pub live: bool
}
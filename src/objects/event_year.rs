use serde::{Serialize, Deserialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct EventYear {
    year: String,
    date_time: String,
    live: bool
}
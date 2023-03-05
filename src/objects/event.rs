use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
pub struct Event {
    name: String,
    slug: String,
}
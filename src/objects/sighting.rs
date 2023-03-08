use serde::Serialize;

use crate::objects::{participant, read};

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all="snake_case")]
pub struct Sighting {
    pub participant: participant::Participant,
    pub read: read::Read,
}

impl Sighting {
    pub fn equals(&self, other: &Sighting) -> bool {
        self.participant.equals(&other.participant) &&
        self.read.equals(&other.read)
    }
}
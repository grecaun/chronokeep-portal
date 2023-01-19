use crate::objects::{participant, read};

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
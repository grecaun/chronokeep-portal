use serde::{Serialize, Deserialize};

use crate::objects::{event_year, event, participant};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GetEventsResponse {
    events: Vec<event::Event>
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GetEventResponse {
    event: event::Event,
    event_years: Vec<event_year::EventYear>
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GetParticipantsResponse {
    participants: Vec<participant::Participant>
}
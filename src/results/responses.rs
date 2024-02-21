use serde::{Serialize, Deserialize};

use crate::{control::socket::requests, objects::{event, event_year}};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GetEventsResponse {
    pub events: Vec<event::Event>
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GetEventResponse {
    pub event: event::Event,
    pub event_years: Vec<event_year::EventYear>
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GetParticipantsResponse {
    pub participants: Vec<requests::RequestParticipant>
}
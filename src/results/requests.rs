use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
pub struct GetParticipantsRequest {
    pub(crate) slug: String,
    pub(crate) year: String
}

#[derive(Serialize, Debug, Clone)]
pub struct GetBibChipsRequest {
    pub(crate) slug: String,
    pub(crate) year: String
}

#[derive(Serialize, Debug, Clone)]
pub struct GetEventRequest {
    pub(crate) slug: String
}
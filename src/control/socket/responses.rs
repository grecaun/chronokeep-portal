use serde::Serialize;

use crate::{objects::setting, network::api};

#[derive(Serialize, Debug)]
#[serde(tag="type", rename_all="camelCase")]
pub struct Readers {
    pub readers: Vec<Reader>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct Reader {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub ip_address: String,
    pub port: u16,
    pub reading: Option<bool>,
    pub connected: Option<bool>,
}

#[derive(Serialize, Debug)]
#[serde(tag="type", rename_all="camelCase")]
pub struct Error {
    pub message: String,
}

#[derive(Serialize, Debug)]
#[serde(tag="type", rename_all="camelCase")]
pub struct Settings {
    pub settings: Vec<setting::Setting>,
}

#[derive(Serialize, Debug)]
#[serde(tag="type", rename_all="camelCase")]
pub struct ApiList {
    pub apis: Vec<api::Api>,
}

#[derive(Serialize, Debug)]
#[serde(tag="type", rename_all="camelCase")]
pub struct Reads {
    pub list: Vec<Read>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct Read {
    pub id: u64,
    pub chip: String,
    pub seconds: u64,
    pub milliseconds: u32,
    pub antenna: u32,
    pub reader: String,
    pub rssi: String
}

#[derive(Serialize, Debug)]
#[serde(tag="type", rename_all="camelCase")]
pub struct Success {
    pub count: usize,
}

#[derive(Serialize, Debug)]
#[serde(tag="type", rename_all="camelCase")]
pub struct Time {
    pub local: String,
    pub utc: String,
}
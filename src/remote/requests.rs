use serde::Serialize;

use crate::objects::{notification::RemoteNotification, read};

#[derive(Serialize, Debug, Clone)]
pub struct UploadReadsRequest {
    pub reads: Vec<read::Read>
}

#[derive(Serialize, Debug, Clone)]
pub struct SaveNotificationRequest {
    pub notification: RemoteNotification
}
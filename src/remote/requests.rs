use serde::Serialize;

use crate::objects::read;

#[derive(Serialize, Debug, Clone)]
pub struct UploadReadsRequest {
    pub(crate) reads: Vec<read::Read>
}
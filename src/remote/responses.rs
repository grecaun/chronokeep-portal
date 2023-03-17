use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct UploadReadsResponse {
    pub(crate) count: usize
}
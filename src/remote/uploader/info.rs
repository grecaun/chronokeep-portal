use crate::remote::uploader::Status;

#[derive(Clone)]
pub struct UploadInfo {
    pub status: Status,
    pub errors: usize,
}

impl UploadInfo {
    pub fn new(
        status: Status,
        errors: usize,
    ) -> Self {
        Self {
            status,
            errors,
        }
    }

    pub fn update_status(
        &mut self,
        status: Status,
        errors: usize,
    ) {
        self.status = status;
        self.errors = errors;
    }
}
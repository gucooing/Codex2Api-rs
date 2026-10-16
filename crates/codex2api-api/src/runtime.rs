//! Gateway view of the shared Redis admission service.
use codex2api_storage::Storage;
pub use codex2api_storage::{AdmissionError as LimitError, ApiLimits as Limits, ApiPermit};

#[derive(Clone)]
pub struct ApiRuntime {
    storage: Storage,
}
impl ApiRuntime {
    pub(crate) fn new(storage: Storage) -> Self {
        Self { storage }
    }
    pub fn current_for_key(&self, key: &str) -> u32 {
        self.storage.api_key_concurrency(key)
    }
    pub fn current_for_user_group(&self, user: &str, group: &str) -> u32 {
        self.storage.api_group_concurrency(user, group)
    }
    pub async fn acquire(
        &self,
        user: &str,
        key: &str,
        group: &str,
        model: &str,
        limits: Limits,
    ) -> Result<ApiPermit, LimitError> {
        self.storage
            .acquire_api_request(user, key, group, model, limits)
            .await
    }
}

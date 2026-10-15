//! Supplier selection and failure recovery, independent of consumer credentials and billing.
//! Callers supply a validated route and keep their own authorization and request ledger.
mod selection;
mod transport;

pub use selection::{
    ScheduleRequest, body_seed, context_required, hash, header_seed, previous_key, select, text,
};
pub use transport::{
    generation_observed, inspect_response, observe_failure, prelude_event, recover_auth,
};

pub type Result<T> = std::result::Result<T, SchedulerError>;

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error(transparent)]
    Storage(#[from] codex2api_storage::StorageError),
    #[error(transparent)]
    Upstream(#[from] codex2api_upstream::UpstreamError),
    #[error("The consumer credential or route is no longer valid")]
    ConsumerInvalid,
    #[error("{message}")]
    Rejected {
        status: http::StatusCode,
        error_type: &'static str,
        message: String,
        code: Option<&'static str>,
    },
}

impl SchedulerError {
    pub fn openai(
        status: http::StatusCode,
        error_type: &'static str,
        message: impl Into<String>,
        code: Option<&'static str>,
    ) -> Self {
        Self::Rejected {
            status,
            error_type,
            message: message.into(),
            code,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SupplierRoute {
    /// Namespaced consumer identity, never a credential or a supplier identity.
    pub owner_id: String,
    pub provider_id: String,
    pub tag_ids: Vec<String>,
}

#[derive(Clone)]
pub struct SupplierContext {
    pub account: std::sync::Arc<codex2api_storage::SupplierAccount>,
    pub reused_affinity: bool,
    pub previous_response_matches: bool,
    pub permit: Option<codex2api_storage::RuntimePermit>,
    pub request: ScheduleRequest,
}

pub fn exhausted() -> SchedulerError {
    SchedulerError::openai(
        http::StatusCode::SERVICE_UNAVAILABLE,
        "server_error",
        "No available supplier account in the configured tag pool. Please retry later.",
        Some("supplier_pool_exhausted"),
    )
}

//! Provider-neutral availability policy. Wire error classification belongs to the adapter.
pub use crate::providers::chatgpt::availability::{
    classify_supplier_failure, quota_available, quota_unavailable_until,
};
use http::HeaderMap;
use serde_json::Value;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupplierFailure {
    Authentication,
    PaymentRequired { code: Option<String> },
    QuotaExhausted { until: i64, code: String },
}
pub fn classify_provider_failure(
    provider: &str,
    status: Option<u16>,
    value: &Value,
    headers: &HeaderMap,
    now: i64,
) -> Option<SupplierFailure> {
    match provider {
        codex2api_core::CHATGPT => classify_supplier_failure(status, value, headers, now),
        codex2api_core::GROK => crate::grok::classify_failure(status, value, headers, now),
        _ => None,
    }
}
impl crate::UpstreamError {
    pub fn supplier_failure(&self, now: i64) -> Option<SupplierFailure> {
        self.supplier_failure_for(codex2api_core::CHATGPT, now)
    }
    pub fn supplier_failure_for(&self, provider: &str, now: i64) -> Option<SupplierFailure> {
        match self {
            Self::Unauthorized | Self::MissingAccessToken(_) => {
                Some(SupplierFailure::Authentication)
            }
            Self::Refresh(e) | Self::Auth(e) if e.permanent_refresh_failure() => {
                Some(SupplierFailure::Authentication)
            }
            Self::GrokAuth(e) if e.permanent_refresh_failure() => {
                Some(SupplierFailure::Authentication)
            }
            Self::Status {
                status,
                body,
                headers,
                ..
            } => classify_provider_failure(
                provider,
                Some(*status),
                &serde_json::from_str(body).unwrap_or_default(),
                headers,
                now,
            ),
            _ => None,
        }
    }
}

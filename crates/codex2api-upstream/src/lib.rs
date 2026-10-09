//! Upstream Codex HTTP client.
//!
//! Talks to official Codex servers with the same application-layer request
//! identity as a logged-in Codex CLI at commit
//! `c1382380de69521303b416720a52f42d51af6248`.
//!
//! Transport uses stock reqwest and the pinned WebSocket transport dependencies.
//! This crate does not spoof TLS/JA3.
//!
//! Public services: `UpstreamPool` selects an isolated account client;
//! `UpstreamClient` sends requests; `Endpoint`, `BackendEndpoint`, and `RealtimeKind`
//! describe official outbound targets. Incoming HTTP routes belong to codex2api-api.
//! Protocol construction lives in request/headers; transport lives in client/websocket/stream.

pub(crate) use providers::chatgpt::backend;
pub use providers::grok;
pub mod providers;
pub use catalog::{codex_model_descriptor, configured_model_descriptor};
pub(crate) use providers::chatgpt::catalog;
pub(crate) use providers::chatgpt::client;
pub fn supported_models() -> Vec<codex2api_core::SupportedModel> {
    catalog::supported_models()
}

mod availability;
pub(crate) use providers::chatgpt::compat;
pub(crate) use providers::chatgpt::endpoint;
mod body;
mod error;
mod outcome;
pub use availability::{
    SupplierFailure, classify_provider_failure, classify_supplier_failure, quota_available,
    quota_unavailable_until,
};
#[derive(Clone, Copy, Debug)]
pub struct SupplierAuthRevision(pub i64);
pub use outcome::{FailureKind, ResponseFailure, ResponseLifecycle, ResponseOutcome};
pub(crate) use providers::chatgpt::headers;
mod pool;
pub(crate) use providers::chatgpt::proxy;
pub(crate) use providers::chatgpt::realtime;
pub(crate) use providers::chatgpt::request;
pub(crate) use providers::chatgpt::routing;
mod stream;
pub(crate) use providers::chatgpt::timezone;
pub(crate) use providers::chatgpt::websocket;

#[cfg(test)]
#[path = "../../codex2api-auth/tests/support/proxy.rs"]
mod proxy_fixture;

pub use backend::BackendEndpoint;
pub use client::{DEFAULT_STREAM_IDLE_TIMEOUT, UpstreamClient, responses_url};
pub use compat::{ChatgptEndpoint, responses_subpath_url};
pub use endpoint::Endpoint;
pub use error::{Result, UpstreamError};
pub use headers::{
    ACCEPT_EVENT_STREAM, CHATGPT_ACCOUNT_ID_HEADER, ORIGINATOR_HEADER, RequestHeaders,
    SESSION_ID_HEADER, THREAD_ID_HEADER, X_CLIENT_REQUEST_ID_HEADER,
    X_CODEX_INSTALLATION_ID_HEADER, X_CODEX_PARENT_THREAD_ID_HEADER, X_CODEX_TURN_METADATA_HEADER,
    X_CODEX_TURN_STATE_HEADER, X_CODEX_WINDOW_ID_HEADER, X_OPENAI_SUBAGENT_HEADER, default_headers,
    insert_header, strip_hop_by_hop_headers,
};
pub use pool::UpstreamPool;
pub use realtime::{RealtimeKind, realtime_call_models, realtime_url};
pub use request::{
    MAX_REQUEST_BYTES, decode_body, normalize_response_identity, serialize_responses_request,
};
pub use request::{RequestMetadata, request_metadata};
pub use routing::{WorkspaceConnection, WorkspaceRoute};
pub use stream::{SseEvent, SseForwardStream, format_sse_event, spawn_sse_forward};
pub use timezone::apply_response_timezone;
pub use websocket::UpstreamWebSocket;

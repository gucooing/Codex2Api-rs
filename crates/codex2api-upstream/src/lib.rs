//! Supplier transport, availability and shared pool scheduling.
//!
//! The ChatGPT provider talks to official Codex servers with the same request
//! identity as a logged-in Codex CLI at commit
//! `83086188ae0614bd882818ff1aa23f3194962012`.
//!
//! Transport uses stock reqwest and the pinned WebSocket transport dependencies.
//! This crate does not spoof TLS/JA3.
//!
//! Public services: `UpstreamPool` selects an isolated account client;
//! `UpstreamClient` sends requests; `Endpoint` and `BackendEndpoint`
//! describe official outbound targets. Incoming client and standard API routes
//! belong to codex2api-oauth and codex2api-api respectively.
//! Protocol construction lives in request/headers; transport lives in client/websocket/stream.

pub use providers::antigravity;
pub(crate) use providers::chatgpt::backend;
pub use providers::grok;
pub mod providers;
pub use catalog::{codex_model_descriptor, configured_model_descriptor};
pub(crate) use providers::chatgpt::catalog;
pub(crate) use providers::chatgpt::client;
pub fn supported_models() -> Vec<codex2api_common::SupportedModel> {
    catalog::supported_models()
}

mod availability;
pub(crate) use providers::chatgpt::endpoint;
pub(crate) use providers::chatgpt::operations;
mod body;
mod error;
mod outcome;
pub mod public_output;
pub mod request_debug;
pub mod request_failure;
pub use availability::{
    SupplierFailure, classify_provider_failure, classify_supplier_failure, quota_available,
    quota_unavailable_until,
};
pub use request_failure::RequestFailureLog;
#[derive(Clone, Copy, Debug)]
pub struct SupplierAuthRevision(pub i64);
pub use outcome::{FailureKind, ResponseFailure, ResponseLifecycle, ResponseOutcome};
pub(crate) use providers::chatgpt::headers;
mod pool;
pub(crate) use providers::chatgpt::proxy;
pub(crate) use providers::chatgpt::request;
pub(crate) use providers::chatgpt::routing;
mod client_ip;
mod stream;
mod timing;
mod transport;
pub use client_ip::{ClientIpPolicy, client_ip};
pub(crate) use providers::chatgpt::timezone;
pub(crate) use providers::chatgpt::websocket;
pub use stream::{buffered_sse_lines, sse_data};
pub use timing::{RequestClock, ResponseTiming, TokenProtocol};
pub use transport::{HttpFuture, HttpTransport, ReqwestTransport};

#[cfg(test)]
#[path = "../../codex2api-suppliers/tests/support/proxy.rs"]
mod proxy_fixture;

pub use backend::BackendEndpoint;
pub use body::{data_url_resolution, request_metadata_value, request_user_agent};
pub use client::{DEFAULT_STREAM_IDLE_TIMEOUT, UpstreamClient, responses_url};
pub use endpoint::Endpoint;
pub use error::{Result, UpstreamError};
pub use headers::{
    ACCEPT_EVENT_STREAM, CHATGPT_ACCOUNT_ID_HEADER, ORIGINATOR_HEADER, RequestHeaders,
    SESSION_ID_HEADER, THREAD_ID_HEADER, X_CLIENT_REQUEST_ID_HEADER,
    X_CODEX_INSTALLATION_ID_HEADER, X_CODEX_PARENT_THREAD_ID_HEADER, X_CODEX_TURN_METADATA_HEADER,
    X_CODEX_TURN_STATE_HEADER, X_CODEX_WINDOW_ID_HEADER, X_OPENAI_SUBAGENT_HEADER, default_headers,
    insert_header, strip_hop_by_hop_headers,
};
pub use operations::{ChatgptEndpoint, responses_subpath_url};
pub use pool::UpstreamPool;
pub use request::{
    MAX_REQUEST_BYTES, decode_body, normalize_response_identity, serialize_responses_request,
};
pub use request::{RequestMetadata, request_metadata};
pub use routing::{WorkspaceConnection, WorkspaceRoute};
pub use stream::{SseEvent, SseForwardStream, format_sse_event, spawn_sse_forward};
pub use timezone::apply_response_timezone;
pub use websocket::UpstreamWebSocket;

mod response;
pub use response::ResponseFormat;

pub mod scheduler;

/// Public protocol adapters; chosen only after supplier selection.
pub mod protocol;

//! Upstream Codex HTTP client.
//!
//! Talks to official Codex servers with the same application-layer request
//! identity as a logged-in Codex CLI at commit
//! `a8964cb1bad67bc26a826fb07d1bef99c6a3f008`.
//!
//! Transport uses stock reqwest and the pinned WebSocket transport dependencies.
//! This crate does not spoof TLS/JA3.
//!
//! Public services: `UpstreamPool` selects an isolated account client;
//! `UpstreamClient` sends requests; `Endpoint`, `BackendEndpoint`, and `RealtimeKind`
//! describe official outbound targets. Incoming HTTP routes belong to codex2api-api.
//! Protocol construction lives in request/headers; transport lives in client/websocket/stream.

mod backend;
mod client;
mod endpoint;
mod error;
mod headers;
mod pool;
mod proxy;
mod realtime;
mod request;
mod stream;
mod timezone;
mod websocket;

#[cfg(test)]
#[path = "../../codex2api-auth/tests/support/proxy.rs"]
mod proxy_fixture;

pub use backend::BackendEndpoint;
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
pub use pool::UpstreamPool;
pub use realtime::{RealtimeKind, realtime_url};
pub use request::{MAX_REQUEST_BYTES, normalize_response_identity};
pub use request::{RequestMetadata, request_metadata};
pub use stream::{SseEvent, SseForwardStream, format_sse_event, spawn_sse_forward};
pub use timezone::apply_response_timezone;
pub use websocket::UpstreamWebSocket;

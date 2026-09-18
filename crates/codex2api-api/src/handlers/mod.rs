//! Incoming HTTP/WebSocket handlers. Route registration belongs in lib.rs.
pub(crate) mod backend;
pub(crate) mod chatgpt;
pub(crate) mod codex;
pub(crate) mod oauth;
pub(crate) mod realtime;
pub(crate) mod system;
pub(crate) mod websocket;

//! Incoming HTTP/WebSocket handlers. Route registration belongs in lib.rs.
pub(crate) mod backend;
pub(crate) mod chatgpt;
pub(crate) mod codex;
pub(crate) mod desktop;
pub(crate) mod desktop_profile;
pub(crate) mod desktop_support;
pub(crate) mod desktop_usage;
pub(crate) mod oauth;
pub(crate) mod oauth_authorize;
mod oauth_jwt;
pub(crate) mod realtime;
pub(crate) mod remote_control;
pub(crate) mod system;
pub(crate) mod websocket;

pub(crate) mod missing;
pub(crate) mod virtual_conversations;
pub(crate) mod virtual_data;
pub(crate) mod virtual_events;

pub(crate) mod family_notices;
pub(crate) mod virtual_tasks;

pub(crate) mod virtual_operations;

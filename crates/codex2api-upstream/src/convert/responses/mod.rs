//! The Responses-family wires: Codex, xAI and OpenAI Responses. Requests are built
//! per downstream format with the dialect's field policy; events are converted into
//! the caller's protocol.
pub mod chat_output;
pub mod output;
pub mod request;
pub mod to_chat;
pub mod to_messages;

pub use request::{Dialect, build};

use crate::convert::EventConversion;
use crate::inbound::{Inbound, Result};
use serde_json::Value;

/// Events of a Responses-family wire delivered to a caller of `inbound`.
pub enum Events {
    /// Responses callers and native clients read the wire's events as they are.
    Native,
    Chat(Box<to_chat::Stream>),
    Messages(to_messages::Stream),
}
impl Events {
    pub fn new(inbound: Inbound, include_usage: bool) -> Self {
        match inbound {
            Inbound::Chat => Self::Chat(Box::new(to_chat::Stream::new(include_usage))),
            Inbound::Messages => Self::Messages(to_messages::Stream::default()),
            _ => Self::Native,
        }
    }
}
impl EventConversion for Events {
    fn event(&mut self, value: Value) -> Result<Vec<Value>> {
        // Official rate-limit notifications are the subscription's own telemetry.
        if matches!(
            value["type"].as_str(),
            Some("codex.rate_limits" | "rate_limits.updated")
        ) {
            return Ok(Vec::new());
        }
        match self {
            Self::Native => Ok(vec![value]),
            Self::Chat(stream) => stream.event(&value),
            Self::Messages(stream) => stream.event(&value),
        }
    }
}

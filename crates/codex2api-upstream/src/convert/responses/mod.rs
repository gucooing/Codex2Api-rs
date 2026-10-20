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

/// Locally generated Responses streams share a monotonically increasing event
/// sequence. Native Responses streams retain the supplier's own sequence.
pub(crate) struct Sequenced {
    inner: Box<dyn EventConversion>,
    next: u64,
}

impl Sequenced {
    pub(crate) fn new(inner: Box<dyn EventConversion>) -> Self {
        Self { inner, next: 0 }
    }

    fn number(&mut self, mut events: Vec<Value>) -> Vec<Value> {
        for event in &mut events {
            event["sequence_number"] = self.next.into();
            self.next += 1;
        }
        events
    }
}

impl EventConversion for Sequenced {
    fn event(&mut self, value: Value) -> Result<Vec<Value>> {
        let events = self.inner.event(value)?;
        Ok(self.number(events))
    }

    fn finish(&mut self) -> Result<Vec<Value>> {
        let events = self.inner.finish()?;
        Ok(self.number(events))
    }
}

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

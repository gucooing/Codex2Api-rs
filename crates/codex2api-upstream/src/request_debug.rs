//! Opt-in, lossless console tracing for AI request and response payloads.
use base64::Engine;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

pub fn enabled() -> bool {
    tracing::enabled!(target: "codex2api::request_io", tracing::Level::DEBUG)
}

#[derive(Clone, Default)]
pub struct RequestTrace(Option<Arc<TraceContext>>);

struct TraceContext {
    id: String,
    transport: &'static str,
    path: String,
    sequence: AtomicU64,
}

impl RequestTrace {
    pub fn new(transport: &'static str, path: &str) -> Self {
        Self(enabled().then(|| {
            Arc::new(TraceContext {
                id: uuid::Uuid::new_v4().to_string(),
                transport,
                path: path.to_owned(),
                sequence: AtomicU64::new(0),
            })
        }))
    }

    pub fn input(&self, bytes: &[u8]) {
        self.payload("input", bytes);
    }

    pub fn output(&self, bytes: &[u8]) {
        self.payload("output", bytes);
    }

    pub fn response_status(&self, status: u16) {
        if let Some(context) = &self.0 {
            tracing::debug!(target: "codex2api::request_io",
                request_id = %context.id, transport = context.transport, path = %context.path,
                status, "AI response status");
        }
    }

    fn payload(&self, direction: &str, bytes: &[u8]) {
        let Some(context) = &self.0 else { return };
        let sequence = context.sequence.fetch_add(1, Ordering::Relaxed);
        // Base64 preserves binary data and UTF-8 characters split across chunks.
        // No truncation, buffering of whole responses, or changes to wire bytes.
        let (encoding, body) = match std::str::from_utf8(bytes) {
            Ok(text) => ("utf8", std::borrow::Cow::Borrowed(text)),
            Err(_) => (
                "base64",
                std::borrow::Cow::Owned(base64::engine::general_purpose::STANDARD.encode(bytes)),
            ),
        };
        tracing::debug!(target: "codex2api::request_io",
            request_id = %context.id, transport = context.transport, path = %context.path,
            direction = %direction, sequence, encoding = %encoding, length = bytes.len(), body = %body,
            "AI request payload");
    }
}

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use eventsource_stream::{Event, Eventsource};
use futures::Stream;
use futures::StreamExt;
use http::header::HeaderMap;
use http::StatusCode;
use reqwest::Response;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::error::{Result, UpstreamError};
use crate::headers::X_CODEX_TURN_STATE_HEADER;

const SSE_CHANNEL_CAPACITY: usize = 1600;
const REQUEST_ID_HEADER: &str = "x-request-id";

/// One SSE event from official Codex `/responses`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
    pub id: String,
    pub retry: Option<u64>,
}

impl From<Event> for SseEvent {
    fn from(event: Event) -> Self {
        Self {
            event: event.event,
            data: event.data,
            id: event.id,
            retry: event.retry.map(|d| d.as_millis() as u64),
        }
    }
}

impl SseEvent {
    /// Reconstruct a wire SSE frame for forwarding to a Codex client.
    pub fn to_sse_frame(&self) -> String {
        format_sse_event(self)
    }
}

pub fn format_sse_event(event: &SseEvent) -> String {
    let mut out = String::new();
    if !event.event.is_empty() {
        out.push_str("event: ");
        out.push_str(&event.event);
        out.push('\n');
    }
    if event.data.is_empty() {
        out.push_str("data:\n");
    } else {
        for line in event.data.split('\n') {
            out.push_str("data: ");
            out.push_str(line);
            out.push('\n');
        }
    }
    if !event.id.is_empty() {
        out.push_str("id: ");
        out.push_str(&event.id);
        out.push('\n');
    }
    if let Some(retry) = event.retry {
        out.push_str("retry: ");
        out.push_str(&retry.to_string());
        out.push('\n');
    }
    out.push('\n');
    out
}

/// Forwarding stream of official Codex SSE events.
///
/// Bytes from the upstream `text/event-stream` body are parsed with
/// `eventsource-stream` (same crate official Codex uses) and yielded as
/// [`SseEvent`]. Dropping the stream cancels the reader task.
pub struct SseForwardStream {
    pub status: StatusCode,
    pub response_headers: HeaderMap,
    pub upstream_request_id: Option<String>,
    rx: mpsc::Receiver<Result<SseEvent>>,
}

impl SseForwardStream {
    pub fn turn_state(&self) -> Option<&str> {
        self.response_headers
            .get(X_CODEX_TURN_STATE_HEADER)
            .and_then(|v| v.to_str().ok())
    }

    pub fn request_id(&self) -> Option<&str> {
        self.upstream_request_id.as_deref()
    }
}

impl Stream for SseForwardStream {
    type Item = Result<SseEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

/// Spawn a task that parses the upstream SSE body and forwards events.
pub fn spawn_sse_forward(response: Response, idle_timeout: Duration) -> SseForwardStream {
    let status = response.status();
    let response_headers = response.headers().clone();
    let upstream_request_id = response_headers
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let (tx, rx) = mpsc::channel(SSE_CHANNEL_CAPACITY);
    let byte_stream = response.bytes_stream();

    tokio::spawn(async move {
        let mut stream = byte_stream.eventsource();
        loop {
            match timeout(idle_timeout, stream.next()).await {
                Ok(Some(Ok(event))) => {
                    if tx.send(Ok(SseEvent::from(event))).await.is_err() {
                        return;
                    }
                }
                Ok(Some(Err(err))) => {
                    let _ = tx.send(Err(UpstreamError::Stream(err.to_string()))).await;
                    return;
                }
                Ok(None) => return,
                Err(_) => {
                    let _ = tx.send(Err(UpstreamError::StreamIdleTimeout)).await;
                    return;
                }
            }
        }
    });

    SseForwardStream {
        status,
        response_headers,
        upstream_request_id,
        rx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_named_event_with_multiline_data() {
        let event = SseEvent {
            event: "response.output_text.delta".into(),
            data: "hello\nworld".into(),
            id: "1".into(),
            retry: None,
        };
        assert_eq!(
            event.to_sse_frame(),
            "event: response.output_text.delta\ndata: hello\ndata: world\nid: 1\n\n"
        );
    }
}

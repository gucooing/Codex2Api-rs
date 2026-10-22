use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use eventsource_stream::{Event, Eventsource};
use futures::Stream;
use futures::StreamExt;
use http::StatusCode;
use http::header::HeaderMap;
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

/// A single data line borrows the frame; only multiline SSE requires a join.
pub fn sse_data(frame: &str) -> std::borrow::Cow<'_, str> {
    let mut lines = frame
        .strip_prefix('\u{feff}')
        .unwrap_or(frame)
        .split(['\r', '\n'])
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start));
    let Some(first) = lines.next() else {
        return std::borrow::Cow::Borrowed("");
    };
    let Some(second) = lines.next() else {
        return std::borrow::Cow::Borrowed(first);
    };
    let mut data = String::with_capacity(first.len() + second.len() + 1);
    data.push_str(first);
    data.push('\n');
    data.push_str(second);
    for line in lines {
        data.push('\n');
        data.push_str(line);
    }
    std::borrow::Cow::Owned(data)
}

/// Incremental SSE line boundaries, shared by parsed events and raw-frame relays.
/// Indices refer to the caller's buffer; consumed bytes are removed only by take.
#[derive(Default)]
struct SseLines {
    scanned: usize,
    start: usize,
    after_cr: bool,
}
impl SseLines {
    fn next(&mut self, bytes: &[u8]) -> Option<std::ops::Range<usize>> {
        while self.scanned < bytes.len() {
            let end = self.scanned;
            let byte = bytes[end];
            self.scanned += 1;
            if self.after_cr && byte == b'\n' {
                self.after_cr = false;
                self.start = self.scanned;
                continue;
            }
            self.after_cr = byte == b'\r';
            if byte == b'\r' || byte == b'\n' {
                let line = self.start..end;
                self.start = self.scanned;
                return Some(line);
            }
        }
        None
    }
    fn take(&mut self) -> usize {
        self.start = 0;
        std::mem::take(&mut self.scanned)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SseChunks {
    Lines,
    Frames,
}

/// Feed complete lines to eventsource-stream. Its streaming parser otherwise
/// rescans an incomplete long data line on every network chunk.
/// Normalize line endings so a final CR dispatches without awaiting another chunk.
pub fn buffered_sse_lines<S, E>(
    source: S,
) -> impl Stream<Item = std::io::Result<bytes::Bytes>> + Send
where
    S: Stream<Item = std::result::Result<bytes::Bytes, E>> + Send,
    E: std::error::Error + Send + Sync + 'static,
{
    split_sse(source, SseChunks::Lines)
}

/// Complete SSE frames with their original bytes, for native client transforms.
/// EOF retains an unfinished frame so each consumer can apply its protocol rules.
pub fn sse_frames<S, E>(source: S) -> impl Stream<Item = std::io::Result<bytes::Bytes>> + Send
where
    S: Stream<Item = std::result::Result<bytes::Bytes, E>> + Send,
    E: std::error::Error + Send + Sync + 'static,
{
    split_sse(source, SseChunks::Frames)
}

fn split_sse<S, E>(
    source: S,
    chunks: SseChunks,
) -> impl Stream<Item = std::io::Result<bytes::Bytes>> + Send
where
    S: Stream<Item = std::result::Result<bytes::Bytes, E>> + Send,
    E: std::error::Error + Send + Sync + 'static,
{
    async_stream::try_stream! {
        let mut source = Box::pin(source);
        let mut buffer = bytes::BytesMut::new();
        let mut lines = SseLines::default();
        let mut first = true;
        while let Some(chunk) = source.next().await {
            buffer.extend_from_slice(&chunk.map_err(std::io::Error::other)?);
            if first && chunks == SseChunks::Lines {
                // SSE ignores one initial UTF-8 BOM. Remove all three bytes before
                // eventsource-stream's single-byte BOM removal can split a character.
                if buffer.len() < 3 && b"\xEF\xBB\xBF".starts_with(&buffer) { continue; }
                if buffer.starts_with(b"\xEF\xBB\xBF") { let _ = buffer.split_to(3); }
                first = false;
            }
            while let Some(line) = lines.next(&buffer) {
                if lines.scanned > crate::MAX_REQUEST_BYTES {
                    Err(std::io::Error::other("Oversized upstream SSE data"))?;
                }
                if chunks == SseChunks::Frames && !line.is_empty() {
                    continue;
                }
                let mut chunk = buffer.split_to(lines.take());
                if chunks == SseChunks::Lines {
                    let _ = chunk.split_to(line.start);
                    let end = chunk.len() - 1;
                    chunk[end] = b'\n';
                }
                yield chunk.freeze();
            }
            if buffer.len() > crate::MAX_REQUEST_BYTES {
                Err(std::io::Error::other("Oversized upstream SSE data"))?;
            }
        }
        if chunks == SseChunks::Lines {
            let _ = buffer.split_to(lines.start);
        }
        if !buffer.is_empty() { yield buffer.freeze(); }
    }
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
        let mut stream = Box::pin(buffered_sse_lines(byte_stream).eventsource());
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

    #[tokio::test]
    async fn buffered_lines_preserve_eventsource_semantics_at_every_chunk_size() {
        use bytes::Bytes;
        use futures::TryStreamExt;
        let input = "\u{feff}: heartbeat\r\nevent: named\rdata: first\rdata: 第二🙂\r\rid: 9\ndata: last\nretry: 300\n\n";
        let source = futures::stream::iter([Ok::<_, std::io::Error>(Bytes::from(
            input.trim_start_matches('\u{feff}'),
        ))]);
        let expected: Vec<_> = source
            .eventsource()
            .map_ok(|e| (e.event, e.data, e.id, e.retry))
            .try_collect()
            .await
            .unwrap();
        assert_eq!(expected.len(), 2);
        for size in [1, 2, 3, 9, input.len()] {
            let chunks = input
                .as_bytes()
                .chunks(size)
                .map(|c| Ok::<_, std::io::Error>(Bytes::copy_from_slice(c)))
                .collect::<Vec<_>>();
            let actual: Vec<_> = buffered_sse_lines(futures::stream::iter(chunks))
                .eventsource()
                .map_ok(|e| (e.event, e.data, e.id, e.retry))
                .try_collect()
                .await
                .unwrap();
            assert_eq!(actual, expected, "chunk size {size}");
        }
    }

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

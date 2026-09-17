//! Client request ledger. Never retain prompts, response text, image bytes or bearer keys.
use axum::body::{Body, Bytes};
use codex2api_storage::{Account, ProxyApiKey, Storage, UsageRecord};
use codex2api_upstream::{Endpoint, RequestMetadata};
use futures::Stream;
use serde::Deserialize;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

fn elapsed(start: Instant) -> i64 {
    start.elapsed().as_millis().min(i64::MAX as u128) as i64
}

#[derive(Clone)]
pub(crate) struct UsageContext {
    storage: Storage,
    account_id: String,
    account_name: String,
    key_id: String,
    key_name: String,
    endpoint: String,
    transport: &'static str,
}

impl UsageContext {
    pub fn new(
        storage: Storage,
        account: &Account,
        key: &ProxyApiKey,
        endpoint: &str,
        transport: &'static str,
    ) -> Self {
        Self {
            storage,
            account_id: account.id.clone(),
            account_name: account
                .display_name
                .as_deref()
                .filter(|s| !s.is_empty())
                .or(account.email.as_deref())
                .unwrap_or(&account.id)
                .into(),
            key_id: key.id.clone(),
            key_name: key
                .name
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(&key.key_prefix)
                .into(),
            endpoint: endpoint.into(),
            transport,
        }
    }

    pub async fn start(
        &self,
        metadata: RequestMetadata,
        start: Instant,
        requested_at_ms: i64,
    ) -> crate::Result<RequestLog> {
        let record = UsageRecord {
            id: uuid::Uuid::new_v4().to_string(),
            account_id: self.account_id.clone(),
            account_name: self.account_name.clone(),
            api_key_id: self.key_id.clone(),
            api_key_name: self.key_name.clone(),
            endpoint: self.endpoint.clone(),
            transport: self.transport.into(),
            model: metadata.model,
            reasoning_effort: metadata.reasoning_effort,
            service_tier: metadata.service_tier,
            image_size: metadata.image_size,
            requested_at_ms,
            status: "in_progress".into(),
            ..Default::default()
        };
        self.storage.insert_usage(&record).await?;
        Ok(RequestLog {
            record: Some(record),
            storage: self.storage.clone(),
            start,
            terminal: false,
        })
    }
}

pub(crate) fn billable(endpoint: Endpoint) -> bool {
    !matches!(endpoint, Endpoint::Models | Endpoint::Usage)
}

pub(crate) struct RequestLog {
    record: Option<UsageRecord>,
    storage: Storage,
    start: Instant,
    terminal: bool,
}

#[derive(Default, Deserialize)]
struct TokenDetails {
    cached_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
}
#[derive(Default, Deserialize)]
struct Tokens {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    input_tokens_details: Option<TokenDetails>,
    output_tokens_details: Option<TokenDetails>,
}
#[derive(Default, Deserialize)]
struct ResponseMetadata {
    id: Option<String>,
    model: Option<String>,
    size: Option<String>,
    usage: Option<Tokens>,
    status: Option<String>,
}
#[derive(Default, Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: Option<String>,
    response: Option<ResponseMetadata>,
    response_id: Option<String>,
    model: Option<String>,
    size: Option<String>,
    usage: Option<Tokens>,
    status: Option<String>,
    error: Option<serde::de::IgnoredAny>,
}

impl RequestLog {
    fn first_byte(&mut self) {
        if let Some(record) = &mut self.record {
            if record.first_byte_ms.is_none() {
                record.first_byte_ms = Some(elapsed(self.start));
            }
        }
    }
    pub fn http_status(&mut self, status: u16) {
        if let Some(record) = &mut self.record {
            record.http_status = Some(i64::from(status));
        }
    }
    fn observe(&mut self, event: &Event) {
        let Some(record) = &mut self.record else {
            return;
        };
        let response = event.response.as_ref();
        if let Some(model) = response
            .and_then(|r| r.model.as_deref())
            .or(event.model.as_deref())
            .filter(|model| !model.trim().is_empty())
        {
            record.actual_model = Some(model.chars().take(256).collect());
        }
        let is_image = record.endpoint.contains("/images/");
        let search = record.endpoint.ends_with("/alpha/search");
        if is_image {
            if let Some(size) = response
                .and_then(|r| r.size.as_deref())
                .or(event.size.as_deref())
            {
                record.image_size = Some(size.chars().take(64).collect());
            }
        } else if !search {
            if let Some(tokens) = response
                .and_then(|r| r.usage.as_ref())
                .or(event.usage.as_ref())
            {
                let valid = |n: Option<i64>| n.filter(|n| *n >= 0);
                record.input_tokens = valid(tokens.input_tokens).or(record.input_tokens);
                record.output_tokens = valid(tokens.output_tokens).or(record.output_tokens);
                record.cached_tokens = valid(
                    tokens
                        .input_tokens_details
                        .as_ref()
                        .and_then(|d| d.cached_tokens),
                )
                .or(record.cached_tokens);
                record.cache_write_tokens = valid(
                    tokens
                        .input_tokens_details
                        .as_ref()
                        .and_then(|d| d.cache_write_tokens),
                )
                .or(record.cache_write_tokens);
                record.reasoning_tokens = valid(
                    tokens
                        .output_tokens_details
                        .as_ref()
                        .and_then(|d| d.reasoning_tokens),
                )
                .or(record.reasoning_tokens);
            }
        }
        let status = response
            .and_then(|r| r.status.as_deref())
            .or(event.status.as_deref());
        let kind = event.kind.as_deref().unwrap_or("");
        if event.error.is_some()
            || matches!(kind, "error" | "response.failed")
            || status == Some("failed")
        {
            record.status = "failed".into();
            self.terminal = true;
        } else if kind == "response.incomplete" || status == Some("incomplete") {
            record.status = "incomplete".into();
            self.terminal = true;
        } else if matches!(kind, "response.completed" | "response.done")
            || status == Some("completed")
        {
            record.status = "completed".into();
            self.terminal = true;
        }
    }
    fn parse(&mut self, bytes: &[u8]) {
        if let Ok(event) = serde_json::from_slice::<Event>(bytes) {
            self.observe(&event);
        }
    }
    pub fn finish(&mut self, status: &str) {
        let Some(mut record) = self.record.take() else {
            return;
        };
        if record.status == "in_progress" {
            record.status = status.into();
        }
        record.total_ms = Some(elapsed(self.start));
        let storage = self.storage.clone();
        tokio::spawn(async move {
            if let Err(error) = storage.finish_usage(&record).await {
                tracing::error!(%error,record_id=%record.id,"failed to finalize usage record");
            }
        });
    }
    pub fn wrap(self, response: reqwest::Response) -> Body {
        let sse = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| h.starts_with("text/event-stream"));
        let success = response.status().is_success();
        let expected_bytes = response.content_length().or_else(|| {
            response
                .headers()
                .get("content-length")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok())
        });
        let mut stream = ObservedStream {
            inner: Box::pin(response.bytes_stream()),
            log: self,
            sse,
            success,
            parser: BodyParser::default(),
            expected_bytes,
            received_bytes: 0,
        };
        if expected_bytes == Some(0) {
            stream.finish_body();
        }
        Body::from_stream(stream)
    }
}
impl Drop for RequestLog {
    fn drop(&mut self) {
        self.finish("interrupted");
    }
}

// Buffer only one SSE event (or one JSON response), with the same bound as API bodies.
const MAX_CAPTURE: usize = codex2api_upstream::MAX_REQUEST_BYTES;
#[derive(Default)]
struct BodyParser {
    line: Vec<u8>,
    data: Vec<u8>,
    skipped: bool,
    after_cr: bool,
    detected_sse: bool,
}
impl BodyParser {
    fn feed(&mut self, bytes: &[u8], sse: bool, log: &mut RequestLog) {
        if !sse && !self.detected_sse {
            if !self.skipped && self.data.len().saturating_add(bytes.len()) <= MAX_CAPTURE {
                self.data.extend_from_slice(bytes);
                // Some official Responses replies omit Content-Type. Detect the SSE
                // framing across chunk boundaries instead of parsing the entire stream as JSON.
                let prefix = self
                    .data
                    .strip_prefix(b"\xef\xbb\xbf")
                    .unwrap_or(&self.data);
                let prefix = prefix.trim_ascii_start();
                if [b"data:".as_slice(), b"event:", b"id:", b"retry:", b":"]
                    .iter()
                    .any(|field| prefix.starts_with(field))
                {
                    self.detected_sse = true;
                    let buffered = std::mem::take(&mut self.data);
                    self.feed(
                        buffered.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&buffered),
                        true,
                        log,
                    );
                }
            } else {
                self.data.clear();
                self.skipped = true;
            }
            return;
        }
        for &byte in bytes {
            if byte == b'\n' && self.after_cr {
                self.after_cr = false;
                continue;
            }
            self.after_cr = byte == b'\r';
            if byte == b'\r' || byte == b'\n' {
                self.line(log);
            } else if self.line.len() + self.data.len() < MAX_CAPTURE {
                self.line.push(byte);
            } else {
                self.skipped = true;
            }
        }
    }
    fn line(&mut self, log: &mut RequestLog) {
        if self.line.is_empty() {
            if !self.skipped && !self.data.is_empty() {
                log.parse(&self.data);
            }
            self.data.clear();
            self.skipped = false;
        } else if let Some(data) = self.line.strip_prefix(b"data:") {
            if !self.skipped {
                if !self.data.is_empty() {
                    self.data.push(b'\n');
                }
                self.data
                    .extend_from_slice(data.strip_prefix(b" ").unwrap_or(data));
            }
        }
        self.line.clear();
    }
    fn end(&mut self, sse: bool, log: &mut RequestLog) {
        if sse || self.detected_sse {
            if !self.line.is_empty() {
                self.line(log);
            }
            self.line(log);
        } else if !self.skipped {
            log.parse(&self.data);
        }
    }
}

struct ObservedStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    log: RequestLog,
    sse: bool,
    success: bool,
    parser: BodyParser,
    expected_bytes: Option<u64>,
    received_bytes: u64,
}
impl ObservedStream {
    fn finish_body(&mut self) {
        if self.log.record.is_none() {
            return;
        }
        self.parser.end(self.sse, &mut self.log);
        self.log.finish(if !self.success {
            "failed"
        } else if (self.sse || self.parser.detected_sse) && !self.log.terminal {
            "incomplete"
        } else {
            "completed"
        });
    }
}
impl Stream for ObservedStream {
    type Item = Result<Bytes, reqwest::Error>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                if !bytes.is_empty() {
                    this.log.first_byte();
                    this.parser.feed(&bytes, this.sse, &mut this.log);
                }
                this.received_bytes = this.received_bytes.saturating_add(bytes.len() as u64);
                // A Content-Length consumer may drop the body without polling EOF.
                if this.expected_bytes == Some(this.received_bytes) {
                    this.finish_body();
                }
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(error))) => {
                this.log.finish("failed");
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                this.finish_body();
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

struct WsSlot {
    response_id: Option<String>,
    log: Option<RequestLog>,
}
pub(crate) struct WsLedger {
    pub context: UsageContext,
    slots: VecDeque<WsSlot>,
}
impl WsLedger {
    pub fn new(context: UsageContext) -> Self {
        Self {
            context,
            slots: VecDeque::new(),
        }
    }
    pub fn push(&mut self, log: Option<RequestLog>) {
        self.slots.push_back(WsSlot {
            response_id: None,
            log,
        });
    }
    pub fn observe(&mut self, bytes: &[u8]) {
        let Ok(event) = serde_json::from_slice::<Event>(bytes) else {
            return;
        };
        let response_id = event
            .response
            .as_ref()
            .and_then(|r| r.id.as_deref())
            .or(event.response_id.as_deref());
        let index = response_id
            .and_then(|id| {
                self.slots
                    .iter()
                    .position(|s| s.response_id.as_deref() == Some(id))
            })
            .or_else(|| {
                if response_id.is_some() {
                    self.slots.iter().position(|s| s.response_id.is_none())
                } else if self.slots.len() == 1 {
                    Some(0)
                } else {
                    None
                }
            });
        let Some(index) = index else { return };
        let slot = &mut self.slots[index];
        if slot.response_id.is_none() {
            slot.response_id = response_id.map(str::to_string);
        }
        if let Some(log) = &mut slot.log {
            log.first_byte();
            log.observe(&event);
        }
        if matches!(
            event.kind.as_deref(),
            Some(
                "response.completed"
                    | "response.done"
                    | "response.failed"
                    | "response.incomplete"
                    | "error"
            )
        ) {
            if let Some(mut log) = self.slots.remove(index).and_then(|s| s.log) {
                log.finish("completed");
            }
        }
    }
}

pub(crate) async fn ws_start(
    ledger: &tokio::sync::Mutex<WsLedger>,
    text: &str,
) -> crate::Result<()> {
    #[derive(Deserialize)]
    struct Reasoning {
        effort: Option<String>,
    }
    #[derive(Deserialize)]
    struct Create {
        #[serde(rename = "type")]
        kind: Option<String>,
        model: Option<String>,
        generate: Option<bool>,
        size: Option<String>,
        reasoning: Option<Reasoning>,
        service_tier: Option<String>,
    }
    let Ok(value) = serde_json::from_str::<Create>(text) else {
        return Ok(());
    };
    if value.kind.as_deref() != Some("response.create") {
        return Ok(());
    }
    let start = Instant::now();
    let now = chrono::Utc::now().timestamp_millis();
    let context = ledger.lock().await.context.clone();
    let log = if value.generate == Some(false) {
        None
    } else {
        Some(
            context
                .start(
                    RequestMetadata {
                        model: value.model.map(|s| s.chars().take(256).collect()),
                        reasoning_effort: value
                            .reasoning
                            .and_then(|r| r.effort)
                            .map(|s| s.chars().take(64).collect()),
                        service_tier: value.service_tier.map(|s| s.chars().take(64).collect()),
                        image_size: value.size.map(|s| s.chars().take(64).collect()),
                        generate: value.generate,
                    },
                    start,
                    now,
                )
                .await?,
        )
    };
    ledger.lock().await.push(log);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, response::Response, routing::post};
    use codex2api_storage::UsageFilter;
    use futures::StreamExt;

    #[tokio::test]
    async fn length_tracking_preserves_partial_disconnects_errors_and_chunked_completion() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("usage.sqlite"))
            .await
            .unwrap();
        let bytes = Bytes::from_static(br#"{"output":"search result","results":[]}"#);
        // No complete JSON or Content-Length has been consumed when the caller cancels.
        let log = context(storage.clone(), "/v1/alpha/search", "http")
            .start(
                RequestMetadata {
                    model: Some("partial".into()),
                    ..Default::default()
                },
                Instant::now(),
                1,
            )
            .await
            .unwrap();
        let stream = futures::stream::iter(vec![
            Ok::<_, std::io::Error>(bytes.slice(..10)),
            Ok(bytes.slice(10..)),
        ]);
        let response = reqwest::Response::from(
            http::Response::builder()
                .header("content-length", bytes.len())
                .body(reqwest::Body::wrap_stream(stream))
                .unwrap(),
        );
        let mut body = log.wrap(response).into_data_stream();
        body.next().await.unwrap().unwrap();
        drop(body);
        for (name, status, length) in [("error", 429, true), ("chunked", 200, false)] {
            let log = context(storage.clone(), "/v1/alpha/search", "http")
                .start(
                    RequestMetadata {
                        model: Some(name.into()),
                        ..Default::default()
                    },
                    Instant::now(),
                    2,
                )
                .await
                .unwrap();
            let stream = futures::stream::iter(vec![
                Ok::<_, std::io::Error>(bytes.slice(..10)),
                Ok(bytes.slice(10..)),
            ]);
            let mut builder = http::Response::builder().status(status);
            if length {
                builder = builder.header("content-length", bytes.len());
            }
            let response =
                reqwest::Response::from(builder.body(reqwest::Body::wrap_stream(stream)).unwrap());
            assert_eq!(
                axum::body::to_bytes(log.wrap(response), usize::MAX)
                    .await
                    .unwrap(),
                bytes
            );
        }
        let log = context(storage.clone(), "/v1/alpha/search", "http")
            .start(
                RequestMetadata {
                    model: Some("empty".into()),
                    ..Default::default()
                },
                Instant::now(),
                3,
            )
            .await
            .unwrap();
        let response = reqwest::Response::from(
            http::Response::builder()
                .status(204)
                .header("content-length", 0)
                .body(Bytes::new())
                .unwrap(),
        );
        drop(log.wrap(response));
        let rows = finalized(&storage, 4).await;
        for (model, status) in [
            ("partial", "interrupted"),
            ("error", "failed"),
            ("chunked", "completed"),
            ("empty", "completed"),
        ] {
            assert_eq!(
                rows.iter()
                    .find(|row| row.model.as_deref() == Some(model))
                    .unwrap()
                    .status,
                status
            );
        }
        storage.close().await;
    }

    #[tokio::test]
    async fn length_delimited_search_finishes_when_client_consumes_the_last_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("usage.sqlite"))
            .await
            .unwrap();
        let log = context(storage.clone(), "/v1/alpha/search", "http")
            .start(RequestMetadata::default(), Instant::now(), 1000)
            .await
            .unwrap();
        let bytes = Bytes::from_static(
            br#"{"output":"search result","encrypted_output":"ciphertext","results":[]}"#,
        );
        let response = reqwest::Response::from(
            http::Response::builder()
                .header("content-type", "application/json")
                .header("content-length", bytes.len())
                .body(bytes.clone())
                .unwrap(),
        );
        let mut stream = log.wrap(response).into_data_stream();
        assert_eq!(stream.next().await.unwrap().unwrap(), bytes);
        // HTTP implementations stop polling after Content-Length bytes; EOF need not be polled.
        drop(stream);
        let rows = finalized(&storage, 1).await;
        assert_eq!(rows[0].status, "completed");
        assert!(rows[0].first_byte_ms.is_some());
        assert!(rows[0].total_ms.is_some());
        assert!(rows[0].input_tokens.is_none());
        storage.close().await;
    }

    #[tokio::test]
    async fn final_sse_usage_is_recorded_without_content_type_and_across_split_prefixes() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("usage.sqlite"))
            .await
            .unwrap();
        let final_event = r#"{"type":"response.completed","response":{"model":"actual-model","status":"completed","usage":{"input_tokens":21,"input_tokens_details":{"cache_write_tokens":0,"cached_tokens":0},"output_tokens":5,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":26}}}"#;
        let bodies=[
            (None,format!("event: response.created\ndata: {{\"type\":\"response.created\",\"response\":{{\"usage\":null}}}}\n\nevent: response.completed\ndata: {final_event}\n\n")),
            (None,format!("\u{feff}: keepalive\r\n\r\ndata: {final_event}\r\n\r\n")),
            (Some("application/octet-stream"),format!("id: 1\nretry: 1000\ndata: {final_event}\n\n")),
            (Some("text/event-stream"),format!("data: {final_event}\n\n")),
            (Some("application/json"),r#"{"model":"actual-model","status":"completed","usage":{"input_tokens":21,"input_tokens_details":{"cache_write_tokens":0,"cached_tokens":0},"output_tokens":5,"output_tokens_details":{"reasoning_tokens":0}}}"#.to_string()),
        ];
        for (i, (content_type, body)) in bodies.into_iter().enumerate() {
            let log = context(storage.clone(), "/v1/responses", "http")
                .start(
                    RequestMetadata {
                        model: Some(format!("case-{i}")),
                        ..Default::default()
                    },
                    Instant::now(),
                    1000 + i as i64,
                )
                .await
                .unwrap();
            let chunks = body
                .as_bytes()
                .chunks(1)
                .map(|bytes| Ok::<_, std::io::Error>(Bytes::copy_from_slice(bytes)))
                .collect::<Vec<_>>();
            let mut builder = http::Response::builder();
            if let Some(content_type) = content_type {
                builder = builder.header("content-type", content_type);
            }
            let response = reqwest::Response::from(
                builder
                    .body(reqwest::Body::wrap_stream(futures::stream::iter(chunks)))
                    .unwrap(),
            );
            let forwarded = axum::body::to_bytes(log.wrap(response), usize::MAX)
                .await
                .unwrap();
            assert_eq!(forwarded.as_ref(), body.as_bytes());
            let rows = finalized(&storage, i + 1).await;
            let row = rows
                .iter()
                .find(|row| row.model.as_deref() == Some(format!("case-{i}").as_str()))
                .unwrap();
            assert_eq!(row.status, "completed");
            assert_eq!(row.input_tokens, Some(21));
            assert_eq!(row.actual_model.as_deref(), Some("actual-model"));
            assert_eq!(row.output_tokens, Some(5));
            assert_eq!(row.cached_tokens, Some(0));
            assert_eq!(row.cache_write_tokens, Some(0));
            assert_eq!(row.reasoning_tokens, Some(0));
        }
        storage.close().await;
    }

    fn context(storage: Storage, endpoint: &str, transport: &'static str) -> UsageContext {
        UsageContext {
            storage,
            account_id: "account-1".into(),
            account_name: "Account One".into(),
            key_id: "key-1".into(),
            key_name: "Key One".into(),
            endpoint: endpoint.into(),
            transport,
        }
    }
    async fn finalized(storage: &Storage, count: usize) -> Vec<UsageRecord> {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let rows = storage
                    .query_usage(&UsageFilter::default())
                    .await
                    .unwrap()
                    .records;
                if rows.len() == count && rows.iter().all(|r| r.status != "in_progress") {
                    break rows;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn http_stream_is_unchanged_and_completion_extracts_usage_across_arbitrary_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("usage.sqlite"))
            .await
            .unwrap();
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(8);
        let rx = std::sync::Arc::new(tokio::sync::Mutex::new(Some(rx)));
        let app = Router::new().route(
            "/responses",
            post(move || {
                let rx = rx.clone();
                async move {
                    Response::builder()
                        .header("content-type", "text/event-stream")
                        .body(Body::from_stream(
                            tokio_stream::wrappers::ReceiverStream::new(
                                rx.lock().await.take().unwrap(),
                            ),
                        ))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut log = context(storage.clone(), "/v1/responses", "http")
            .start(
                RequestMetadata {
                    model: Some("requested".into()),
                    reasoning_effort: Some("xhigh".into()),
                    service_tier: Some("ultrafast".into()),
                    ..Default::default()
                },
                Instant::now(),
                12345,
            )
            .await
            .unwrap();
        let response = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .post(format!("http://{addr}/responses"))
            .send()
            .await
            .unwrap();
        log.http_status(200);
        let mut stream = log.wrap(response).into_data_stream();
        let first=Bytes::from_static(b": heartbeat\r\n\r\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"r1\"}}\r\n\r\n");
        tx.send(Ok(first.clone())).await.unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            first
        );
        // The client receives the first bytes while upstream is still open.
        let completed=b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"model\":\"actual\",\"usage\":{\"input_tokens\":100,\"output_tokens\":20,\"input_tokens_details\":{\"cached_tokens\":40,\"cache_write_tokens\":5},\"output_tokens_details\":{\"reasoning_tokens\":10}}}}\r\n\r\ndata: [DONE]\r\n\r\n";
        for chunk in completed.chunks(61) {
            tx.send(Ok(Bytes::copy_from_slice(chunk))).await.unwrap();
        }
        drop(tx);
        let mut received = Vec::new();
        while let Some(chunk) = stream.next().await {
            received.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(received, completed);
        let rows = finalized(&storage, 1).await;
        let row = &rows[0];
        assert_eq!(row.input_tokens, Some(100));
        assert_eq!(row.output_tokens, Some(20));
        assert_eq!(row.cached_tokens, Some(40));
        assert_eq!(row.cache_write_tokens, Some(5));
        assert_eq!(row.reasoning_tokens, Some(10));
        assert_eq!(row.reasoning_effort.as_deref(), Some("xhigh"));
        assert_eq!(row.service_tier.as_deref(), Some("ultrafast"));
        assert_eq!(row.model.as_deref(), Some("requested"));
        assert_eq!(row.actual_model.as_deref(), Some("actual"));
        assert_eq!(row.status, "completed");
        assert_eq!(row.requested_at_ms, 12345);
        assert!(row.first_byte_ms.is_some());
        assert!(row.total_ms >= row.first_byte_ms);
        server.abort();
        storage.close().await;
    }

    #[tokio::test]
    async fn websocket_records_each_generation_skips_warmups_and_keeps_missing_usage_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("usage.sqlite"))
            .await
            .unwrap();
        let ledger = tokio::sync::Mutex::new(WsLedger::new(context(
            storage.clone(),
            "/v1/responses",
            "websocket",
        )));
        ws_start(
            &ledger,
            r#"{"type":"response.create","generate":false,"model":"warmup"}"#,
        )
        .await
        .unwrap();
        ledger
            .lock()
            .await
            .observe(br#"{"type":"response.created","response":{"id":"warmup"}}"#);
        ledger
            .lock()
            .await
            .observe(br#"{"type":"response.completed","response":{"id":"warmup"}}"#);
        assert_eq!(
            storage
                .query_usage(&UsageFilter::default())
                .await
                .unwrap()
                .total,
            0
        );
        ws_start(
            &ledger,
            r#"{"type":"response.create","model":"one","reasoning":{"effort":"high"},"service_tier":"priority"}"#,
        )
        .await
        .unwrap();
        ledger
            .lock()
            .await
            .observe(br#"{"type":"response.created","response":{"id":"r1","model":"actual-one"}}"#);
        ws_start(
            &ledger,
            r#"{"type":"response.create","model":"two","reasoning":{"effort":"low"}}"#,
        )
        .await
        .unwrap();
        ledger
            .lock()
            .await
            .observe(br#"{"type":"response.created","response":{"id":"r2"}}"#);
        ledger.lock().await.observe(br#"{"type":"response.completed","response":{"id":"r2","model":"actual-two","usage":{"input_tokens":0,"output_tokens":0}}}"#);
        ledger
            .lock()
            .await
            .observe(br#"{"type":"response.failed","response":{"id":"r1"}}"#);
        ws_start(
            &ledger,
            r#"{"type":"response.create","model":"interrupted"}"#,
        )
        .await
        .unwrap();
        drop(ledger);
        let rows = finalized(&storage, 3).await;
        let one = rows
            .iter()
            .find(|r| r.model.as_deref() == Some("one"))
            .unwrap();
        assert_eq!(one.status, "failed");
        assert_eq!(one.actual_model.as_deref(), Some("actual-one"));
        assert_eq!(one.input_tokens, None);
        assert_eq!(one.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(one.service_tier.as_deref(), Some("priority"));
        let two = rows
            .iter()
            .find(|r| r.model.as_deref() == Some("two"))
            .unwrap();
        assert_eq!(two.input_tokens, Some(0));
        assert_eq!(two.actual_model.as_deref(), Some("actual-two"));
        assert!(
            rows.iter()
                .find(|r| r.model.as_deref() == Some("interrupted"))
                .unwrap()
                .actual_model
                .is_none()
        );
        assert_eq!(two.reasoning_effort.as_deref(), Some("low"));
        assert_eq!(
            rows.iter()
                .find(|r| r.model.as_deref() == Some("interrupted"))
                .unwrap()
                .status,
            "interrupted"
        );
        storage.close().await;
    }

    #[tokio::test]
    async fn json_images_search_errors_and_dropped_bodies_are_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("usage.sqlite"))
            .await
            .unwrap();
        for (endpoint, json) in [
            (
                "/v1/images/generations",
                r#"{"size":"1536x1024","data":[{"b64_json":"secret-image"}]}"#,
            ),
            (
                "/v1/alpha/search",
                r#"{"usage":{"input_tokens":900},"output":"private search text"}"#,
            ),
            ("/v1/responses", r#"{"error":{"message":"failed"}}"#),
        ] {
            let mut log = context(storage.clone(), endpoint, "http")
                .start(RequestMetadata::default(), Instant::now(), 1000)
                .await
                .unwrap();
            log.first_byte();
            log.parse(json.as_bytes());
            log.finish("completed");
            assert!(log.record.is_none());
            drop(log);
        }
        let log = context(storage.clone(), "/v1/responses", "http")
            .start(
                RequestMetadata {
                    model: Some("disconnect".into()),
                    ..Default::default()
                },
                Instant::now(),
                1000,
            )
            .await
            .unwrap();
        drop(log);
        let rows = finalized(&storage, 4).await;
        assert_eq!(
            rows.iter()
                .find(|r| r.endpoint.contains("images"))
                .unwrap()
                .image_size
                .as_deref(),
            Some("1536x1024")
        );
        assert_eq!(
            rows.iter()
                .find(|r| r.endpoint.contains("search"))
                .unwrap()
                .input_tokens,
            None
        );
        assert_eq!(
            rows.iter()
                .find(|r| r.model.as_deref() == Some("disconnect"))
                .unwrap()
                .status,
            "interrupted"
        );
        assert!(rows.iter().any(|r| r.status == "failed"));
        storage.close().await;
    }
}

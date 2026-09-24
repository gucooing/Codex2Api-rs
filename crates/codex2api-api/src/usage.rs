//! Client request ledger. Never retain prompts, response text, image bytes or bearer keys.
use crate::execution::ExecutionContext;
use axum::body::{Body, Bytes};
use codex2api_storage::{Storage, UsageRecord};
use codex2api_upstream::{Endpoint, RequestMetadata};
use futures::Stream;
use serde::Deserialize;
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Instant,
};

fn elapsed(start: Instant) -> i64 {
    start.elapsed().as_millis().min(i64::MAX as u128) as i64
}

pub(crate) fn billable(endpoint: Endpoint) -> bool {
    !matches!(
        endpoint,
        Endpoint::Models | Endpoint::Usage | Endpoint::InputTokens
    )
}

pub(crate) struct RequestLog {
    record: Option<UsageRecord>,
    storage: Storage,
    start: Instant,
    terminal: bool,
    authoritative_model: bool,
    client_stopped: Arc<AtomicBool>,
}
impl RequestLog {
    pub(crate) async fn begin(
        storage: Storage,
        record: UsageRecord,
        start: Instant,
    ) -> crate::Result<Self> {
        storage.insert_usage(&record).await?;
        Ok(Self {
            storage,
            record: Some(record),
            start,
            terminal: false,
            authoritative_model: false,
            client_stopped: Arc::new(AtomicBool::new(false)),
        })
    }
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
    headers: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
    incomplete_details: Option<serde_json::Value>,
    size: Option<String>,
    usage: Option<Tokens>,
    status: Option<String>,
    service_tier: Option<String>,
    data: Option<Vec<ImageOutput>>,
}
#[derive(Default, Deserialize)]
struct ImageOutput {
    b64_json: Option<String>,
    url: Option<String>,
    size: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    error: Option<serde::de::IgnoredAny>,
}
#[derive(Default, Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: Option<String>,
    response: Option<ResponseMetadata>,
    response_id: Option<String>,
    model: Option<String>,
    headers: Option<serde_json::Value>,
    code: Option<String>,
    message: Option<String>,
    size: Option<String>,
    usage: Option<Tokens>,
    // Error envelopes use a numeric HTTP status; response events use a string.
    status: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
    detail: Option<serde_json::Value>,
    service_tier: Option<String>,
    data: Option<Vec<ImageOutput>>,
}

fn reported_model(headers: &serde_json::Value) -> Option<String> {
    fn string(value: &serde_json::Value) -> Option<&str> {
        match value {
            serde_json::Value::String(value) => Some(value),
            serde_json::Value::Array(values) => values.first().and_then(string),
            _ => None,
        }
    }
    headers.as_object()?.iter().find_map(|(name, value)| {
        (name.eq_ignore_ascii_case("openai-model") || name.eq_ignore_ascii_case("x-openai-model"))
            .then(|| string(value))
            .flatten()
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().chars().take(256).collect())
    })
}

fn request_id(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}

fn reported_request_id(headers: &serde_json::Value) -> Option<String> {
    headers.as_object()?.iter().find_map(|(name, value)| {
        if !matches!(
            name.to_ascii_lowercase().as_str(),
            "x-request-id" | "x-oai-request-id" | "x-openai-request-id"
        ) {
            return None;
        }
        let value = value
            .as_str()
            .or_else(|| value.as_array()?.first()?.as_str())?;
        request_id(value)
    })
}

fn response_request_id(headers: &http::HeaderMap) -> Option<String> {
    ["x-request-id", "x-oai-request-id", "x-openai-request-id"]
        .into_iter()
        .find_map(|name| headers.get(name).and_then(|value| value.to_str().ok()))
        .and_then(request_id)
}

fn error_text(value: &str, limit: usize) -> String {
    let mut hide_next = false;
    value
        .split_whitespace()
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            let secret = hide_next
                || lower.starts_with("sk-")
                || lower.starts_with("c2a_")
                || lower.starts_with("eyj");
            hide_next = lower.trim_end_matches(':') == "bearer";
            if secret { "[REDACTED]" } else { word }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

fn complete_failure_reason(record: &mut UsageRecord) {
    if record.error_message.is_some() {
        return;
    }
    let (code, message) = match record.status.as_str() {
        "failed" if record.http_status.is_some_and(|s| s >= 400) => (
            "upstream_http_error",
            format!("上游返回 HTTP {}", record.http_status.unwrap()),
        ),
        "failed" => ("upstream_failed", "上游返回失败事件，未提供错误说明".into()),
        "incomplete" => (
            "incomplete_response",
            match record.error_code.as_deref() {
                Some("max_output_tokens") => "达到输出 Token 上限".into(),
                Some("content_filter") => "上游内容过滤中止生成".into(),
                Some(reason) => reason.into(),
                None => "上游响应结束，但未收到完成事件".into(),
            },
        ),
        "interrupted" => (
            "request_aborted",
            "请求处理被中止，未收到正常完成结果".into(),
        ),
        _ => return,
    };
    record.error_code.get_or_insert_with(|| code.into());
    record.error_message = Some(message);
}

impl RequestLog {
    pub fn response_headers(&mut self, headers: &http::HeaderMap) {
        if let Some(id) = response_request_id(headers)
            && let Some(record) = &mut self.record
        {
            record.upstream_request_id = Some(id);
        }
        if let Some(model) = ["openai-model", "x-openai-model"]
            .into_iter()
            .find_map(|name| headers.get(name).and_then(|value| value.to_str().ok()))
            .filter(|value| !value.trim().is_empty())
            && let Some(record) = &mut self.record
        {
            record.actual_model = Some(model.trim().chars().take(256).collect());
            self.authoritative_model = true;
        }
    }
    pub fn failure(&mut self, code: &str, message: &str) {
        if let Some(record) = &mut self.record {
            record.error_code = Some(error_text(code, 128));
            record.error_message = Some(error_text(message, 512));
        }
    }
    pub fn upstream_failure(&mut self, error: &codex2api_upstream::UpstreamError) {
        use codex2api_upstream::UpstreamError as E;
        match error {
            E::Status { status, body } => {
                self.http_status(*status);
                let mut parser = BodyParser::default();
                parser.feed(body.as_bytes(), false, self);
                parser.end(false, self);
            }
            E::Unauthorized => {
                self.http_status(401);
                self.failure("upstream_unauthorized", "上游授权失效（HTTP 401）");
            }
            E::Http(error) if error.is_timeout() => {
                self.failure("upstream_timeout", "上游请求超时")
            }
            E::Http(error) if error.is_connect() => {
                self.failure("upstream_connection_failed", "无法连接上游服务")
            }
            E::Http(_) => self.failure("upstream_transport_error", "上游通信失败"),
            E::Refresh(_) | E::Auth(_) => {
                self.failure("upstream_auth_failed", "上游授权或令牌刷新失败")
            }
            E::MissingAccessToken(_) => {
                self.failure("upstream_auth_missing", "供应账户没有可用授权")
            }
            _ => self.failure("upstream_request_failed", "上游请求未能完成"),
        }
        self.finish("failed");
    }
    fn first_byte(&mut self) {
        if let Some(record) = &mut self.record
            && record.first_byte_ms.is_none()
        {
            record.first_byte_ms = Some(elapsed(self.start));
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
        if let Some(id) = response
            .and_then(|r| r.headers.as_ref())
            .and_then(reported_request_id)
            .or_else(|| event.headers.as_ref().and_then(reported_request_id))
        {
            record.upstream_request_id = Some(id);
        }
        if let Some(tier) = response
            .and_then(|r| r.service_tier.as_ref())
            .or(event.service_tier.as_ref())
        {
            record.service_tier = Some(tier.clone());
        }
        if let Some(model) = response
            .and_then(|r| r.headers.as_ref())
            .and_then(reported_model)
            .or_else(|| event.headers.as_ref().and_then(reported_model))
        {
            record.actual_model = Some(model);
            self.authoritative_model = true;
        } else if !self.authoritative_model
            && let Some(model) = response
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
            if let Some(images) = response
                .and_then(|r| r.data.as_ref())
                .or(event.data.as_ref())
            {
                let mut counts = std::collections::BTreeMap::<Option<String>, i64>::new();
                for item in images {
                    if item.error.is_some()
                        || !(item.b64_json.as_ref().is_some_and(|s| !s.is_empty())
                            || item.url.as_ref().is_some_and(|s| !s.is_empty()))
                    {
                        continue;
                    }
                    let dimensions = item.width.zip(item.height).and_then(|(w, h)| {
                        codex2api_storage::image_resolution(&format!("{w}x{h}"))
                    });
                    let resolution = dimensions
                        .or_else(|| {
                            item.size
                                .as_deref()
                                .and_then(codex2api_storage::image_resolution)
                        })
                        .or_else(|| {
                            record
                                .image_size
                                .as_deref()
                                .and_then(codex2api_storage::image_resolution)
                        })
                        .or_else(|| {
                            item.size
                                .as_deref()
                                .or(record.image_size.as_deref())
                                .and_then(codex2api_storage::image_resolution_tier)
                        });
                    *counts.entry(resolution).or_default() += 1;
                }
                let usage = counts
                    .into_iter()
                    .map(|(resolution, count)| codex2api_storage::ImageUsage { resolution, count })
                    .collect::<Vec<_>>();
                record.image_count = Some(usage.iter().map(|item| item.count).sum());
                record.image_usage_json = serde_json::to_string(&usage).ok();
            } else if record.image_usage_json.is_none()
                && (event.error.is_some()
                    || event.kind.as_deref() == Some("response.failed")
                    || response
                        .and_then(|r| r.status.as_deref())
                        .or_else(|| event.status.as_ref().and_then(serde_json::Value::as_str))
                        == Some("failed"))
            {
                // Only an explicit upstream failure confirms zero returned images.
                // Truncated or unreadable responses retain unknown usage.
                record.image_count = Some(0);
                record.image_usage_json = Some("[]".into());
            }
        } else if !search
            && let Some(tokens) = response
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
        let status = response
            .and_then(|r| r.status.as_deref())
            .or_else(|| event.status.as_ref().and_then(serde_json::Value::as_str));
        let kind = event.kind.as_deref().unwrap_or("");
        let http_error = record.http_status.is_some_and(|status| status >= 400);
        let error = response
            .and_then(|r| r.error.as_ref())
            .or(event.error.as_ref())
            .or_else(|| event.detail.as_ref().filter(|_| http_error));
        fn nonempty(value: &serde_json::Value) -> Option<&str> {
            value.as_str().filter(|value| !value.trim().is_empty())
        }
        let (code, message) = if let Some(error) = error {
            let code = error
                .get("code")
                .and_then(nonempty)
                .or_else(|| error.get("type").and_then(nonempty));
            let message = error
                .get("message")
                .and_then(nonempty)
                .or_else(|| nonempty(error));
            (code, message)
        } else if kind == "error" || http_error {
            (
                event.code.as_deref().filter(|v| !v.trim().is_empty()),
                event.message.as_deref().filter(|v| !v.trim().is_empty()),
            )
        } else {
            (None, None)
        };
        // Later metadata events must not erase an already reported protocol error.
        if let Some(code) = code {
            record.error_code = Some(error_text(code, 128));
        }
        if let Some(message) = message {
            record.error_message = Some(error_text(message, 512));
        }
        if event.error.is_some()
            || response.is_some_and(|r| r.error.is_some())
            || matches!(kind, "error" | "response.failed")
            || status == Some("failed")
        {
            record.status = "failed".into();
            self.terminal = true;
        } else if kind == "response.incomplete" || status == Some("incomplete") {
            record.status = "incomplete".into();
            record.error_code = response
                .and_then(|r| r.incomplete_details.as_ref())
                .and_then(|v| v.get("reason"))
                .and_then(serde_json::Value::as_str)
                .map(|v| error_text(v, 128));
            self.terminal = true;
        } else if kind == "response.cancelled" || status == Some("cancelled") {
            record.status = "client_stopped".into();
            self.terminal = true;
        } else if matches!(kind, "response.completed" | "response.done")
            || status == Some("completed")
        {
            record.status = "completed".into();
            self.terminal = true;
        }
    }
    fn parse(&mut self, bytes: &[u8]) {
        if bytes.trim_ascii() == b"[DONE]"
            && self
                .record
                .as_ref()
                .is_some_and(|r| r.endpoint == "/backend-api/f/conversation")
        {
            if let Some(record) = &mut self.record
                && record.status == "in_progress"
            {
                record.status = "completed".into();
            }
            self.terminal = true;
            return;
        }
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
        complete_failure_reason(&mut record);
        record.total_ms = Some(elapsed(self.start));
        let storage = self.storage.clone();
        tokio::spawn(async move {
            if let Err(error) = storage.finish_usage(&record).await {
                tracing::error!(%error,record_id=%record.id,"failed to finalize usage record");
            }
        });
    }
    pub fn wrap(self, response: reqwest::Response) -> Body {
        self.wrap_with(response, |body| body)
    }
    pub fn wrap_with(
        mut self,
        response: reqwest::Response,
        transform: impl FnOnce(Body) -> Body,
    ) -> Body {
        self.response_headers(response.headers());
        self.http_status(response.status().as_u16());
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
        let client_stopped = self.client_stopped.clone();
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
        Body::from_stream(ClientBody {
            inner: transform(Body::from_stream(stream)).into_data_stream(),
            client_stopped,
            ended: false,
        })
    }
}
impl Drop for RequestLog {
    fn drop(&mut self) {
        let status = if self.client_stopped.load(Ordering::Acquire) {
            if self
                .record
                .as_ref()
                .is_some_and(|record| record.http_status.is_some_and(|code| code >= 400))
            {
                "failed"
            } else {
                "client_stopped"
            }
        } else {
            "interrupted"
        };
        self.finish(status);
    }
}

// Only the outermost response body can identify downstream cancellation. A
// server-side transform can also drop its input after an error; that is not a
// client stop. Drop runs before fields are released and the ledger is finalized.
struct ClientBody {
    inner: axum::body::BodyDataStream,
    client_stopped: Arc<AtomicBool>,
    ended: bool,
}
impl Stream for ClientBody {
    type Item = Result<Bytes, axum::Error>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_next(cx);
        if matches!(result, Poll::Ready(None | Some(Err(_)))) {
            this.ended = true;
        }
        result
    }
}
impl Drop for ClientBody {
    fn drop(&mut self) {
        if !self.ended && !std::thread::panicking() {
            self.client_stopped.store(true, Ordering::Release);
        }
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
        } else if let Some(data) = self.line.strip_prefix(b"data:")
            && !self.skipped
        {
            if !self.data.is_empty() {
                self.data.push(b'\n');
            }
            self.data
                .extend_from_slice(data.strip_prefix(b" ").unwrap_or(data));
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
                this.log.failure(
                    if error.is_timeout() {
                        "upstream_timeout"
                    } else {
                        "upstream_stream_error"
                    },
                    if error.is_timeout() {
                        "上游响应流超时"
                    } else {
                        "上游响应流读取失败，连接已断开"
                    },
                );
                if let Some(record) = &this.log.record {
                    let storage = this.log.storage.clone();
                    let id = record.account_id.clone();
                    tokio::spawn(async move {
                        if let Err(error) = storage
                            .record_supplier_error(&id, "ChatGPT 官方响应流读取失败")
                            .await
                        {
                            tracing::error!(%error, "failed to persist supplier stream failure");
                        }
                    });
                }
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
    pub context: ExecutionContext,
    realtime_model: Option<String>,
    pub transcription_model: Option<String>,
    slots: VecDeque<WsSlot>,
    initial_headers: Option<http::HeaderMap>,
}
impl WsLedger {
    pub fn new(context: ExecutionContext) -> Self {
        Self {
            context,
            realtime_model: None,
            transcription_model: None,
            slots: VecDeque::new(),
            initial_headers: None,
        }
    }
    pub fn realtime(context: ExecutionContext, model: String) -> Self {
        Self {
            context,
            realtime_model: Some(model),
            transcription_model: None,
            slots: VecDeque::new(),
            initial_headers: None,
        }
    }
    pub async fn authorize_realtime(&self) -> crate::Result<()> {
        if let Some(model) = &self.transcription_model {
            self.context
                .authorize(
                    &RequestMetadata {
                        model: Some(model.clone()),
                        ..Default::default()
                    },
                    true,
                )
                .await?;
        }
        self.context
            .authorize(
                &RequestMetadata {
                    model: self.realtime_model.clone(),
                    ..Default::default()
                },
                false,
            )
            .await
    }
    pub fn response_headers(&mut self, headers: &http::HeaderMap) {
        let mut initial = headers.clone();
        // The upgrade request ID belongs to the connection, not a generation.
        for name in ["x-request-id", "x-oai-request-id", "x-openai-request-id"] {
            initial.remove(name);
        }
        self.initial_headers = Some(initial);
    }
    pub async fn fail_inflight(&mut self, code: &str, message: &str) -> crate::Result<()> {
        for mut slot in self.slots.drain(..) {
            if let Some(log) = &mut slot.log {
                log.failure(code, message);
                if let Some(mut record) = log.record.take() {
                    record.status = "failed".into();
                    record.total_ms = Some(elapsed(log.start));
                    log.storage.finish_usage(&record).await?;
                }
            }
        }
        Ok(())
    }
    pub async fn client_stopped(&mut self) -> crate::Result<()> {
        for mut slot in self.slots.drain(..) {
            if let Some(log) = &mut slot.log
                && let Some(mut record) = log.record.take()
            {
                if record.status == "in_progress" {
                    record.status = "client_stopped".into();
                }
                complete_failure_reason(&mut record);
                record.total_ms = Some(elapsed(log.start));
                log.storage.finish_usage(&record).await?;
            }
        }
        Ok(())
    }
    pub fn push(&mut self, mut log: Option<RequestLog>) {
        if let Some(log) = &mut log
            && let Some(headers) = self.initial_headers.take()
        {
            log.response_headers(&headers);
        }
        self.slots.push_back(WsSlot {
            response_id: None,
            log,
        });
    }
    pub async fn observe(&mut self, bytes: &[u8]) -> crate::Result<()> {
        self.observe_authorized(bytes, true).await
    }
    /// Settle already-started work even when its credential was revoked in flight.
    pub async fn observe_existing(&mut self, bytes: &[u8]) -> crate::Result<()> {
        self.observe_authorized(bytes, false).await
    }
    async fn observe_authorized(&mut self, bytes: &[u8], allow_new: bool) -> crate::Result<()> {
        let Ok(event) = serde_json::from_slice::<Event>(bytes) else {
            return Ok(());
        };
        // VAD may create a Realtime response without a client response.create.
        if allow_new
            && self.context.endpoint == "/v1/realtime"
            && event.kind.as_deref() == Some("response.created")
            && self.slots.iter().all(|s| s.response_id.is_some())
        {
            let log = self
                .context
                .start(
                    RequestMetadata {
                        model: event
                            .response
                            .as_ref()
                            .and_then(|r| r.model.clone())
                            .or_else(|| self.realtime_model.clone()),
                        ..Default::default()
                    },
                    Instant::now(),
                    chrono::Utc::now().timestamp_millis(),
                )
                .await?;
            self.push(Some(log));
        }
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
        let Some(index) = index else { return Ok(()) };
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
                    | "response.cancelled"
                    | "error"
            )
        ) && let Some(mut log) = self.slots.remove(index).and_then(|s| s.log)
            && let Some(mut record) = log.record.take()
        {
            if record.status == "in_progress" {
                record.status = "completed".into();
            }
            complete_failure_reason(&mut record);
            record.total_ms = Some(elapsed(log.start));
            log.storage.finish_usage(&record).await?;
        }
        Ok(())
    }
}

pub(crate) async fn realtime_start(
    ledger: &tokio::sync::Mutex<WsLedger>,
    text: Option<&str>,
) -> crate::Result<()> {
    #[derive(Default, Deserialize)]
    struct Model {
        model: Option<String>,
        audio: Option<Audio>,
    }
    #[derive(Deserialize)]
    struct Audio {
        input: Option<AudioInput>,
    }
    #[derive(Deserialize)]
    struct AudioInput {
        transcription: Option<Transcription>,
    }
    #[derive(Deserialize)]
    struct Transcription {
        model: Option<String>,
    }
    #[derive(Deserialize)]
    struct Frame {
        #[serde(rename = "type")]
        kind: String,
        model: Option<String>,
        session: Option<Model>,
        response: Option<Model>,
    }
    let frame: Option<Frame> = text
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| crate::ApiError::bad_request("Invalid Realtime frame fields."))?;
    let mut ledger = ledger.lock().await;
    let Some(frame) = frame else {
        return ledger.authorize_realtime().await;
    };
    // Only the documented session or response field selects the generation model.
    // A stray top-level model must not change the model inherited by audio frames.
    if frame.model.is_some()
        || (frame.kind != "session.update" && frame.session.is_some())
        || (frame.kind != "response.create" && frame.response.is_some())
    {
        return Err(crate::ApiError::bad_request(
            "Realtime model is outside its session or response field.",
        ));
    }
    let transcription = frame
        .session
        .as_ref()
        .and_then(|s| s.audio.as_ref())
        .and_then(|a| a.input.as_ref())
        .and_then(|i| i.transcription.as_ref())
        .map(|t| {
            t.model.clone().ok_or_else(|| {
                crate::ApiError::bad_request("An explicit transcription model is required.")
            })
        })
        .transpose()?;
    let clears_transcription = text
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .is_some_and(|value| {
            value
                .pointer("/session/audio/input/transcription")
                .is_some_and(serde_json::Value::is_null)
        });
    let previous_transcription = if clears_transcription {
        None
    } else {
        ledger.transcription_model.as_ref()
    };
    if let Some(model) = transcription.as_ref().or(previous_transcription) {
        ledger
            .context
            .authorize(
                &RequestMetadata {
                    model: Some(model.clone()),
                    ..Default::default()
                },
                true,
            )
            .await?;
    }
    let model = frame
        .session
        .and_then(|s| s.model)
        .or_else(|| frame.response.and_then(|r| r.model))
        .or_else(|| ledger.realtime_model.clone());
    let metadata = RequestMetadata {
        model,
        ..Default::default()
    };
    ledger.context.authorize(&metadata, false).await?;
    if frame.kind == "session.update" {
        ledger.realtime_model = metadata.model.clone();
        if transcription.is_some() || clears_transcription {
            ledger.transcription_model = transcription;
        }
    }
    if frame.kind == "response.create" {
        let log = ledger
            .context
            .start(
                metadata,
                Instant::now(),
                chrono::Utc::now().timestamp_millis(),
            )
            .await?;
        ledger.push(Some(log));
    }
    Ok(())
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
    let value: Create = serde_json::from_str(text)
        .map_err(|_| crate::ApiError::bad_request("Invalid Responses frame fields."))?;
    if value.kind.as_deref() != Some("response.create") {
        return Ok(());
    }
    let start = Instant::now();
    let now = chrono::Utc::now().timestamp_millis();
    let context = ledger.lock().await.context.clone();
    let log = if value.generate == Some(false) {
        context
            .authorize(
                &RequestMetadata {
                    model: value.model.clone(),
                    ..Default::default()
                },
                true,
            )
            .await?;
        None
    } else {
        Some(
            context
                .start(
                    RequestMetadata {
                        model: value.model,
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
    async fn client_stop_is_successful_and_settles_only_reported_usage() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("stop.sqlite")).await.unwrap();
        let usage = b"data: {\"type\":\"response.in_progress\",\"response\":{\"usage\":{\"input_tokens\":100,\"output_tokens\":20,\"input_tokens_details\":{\"cached_tokens\":40},\"output_tokens_details\":{\"reasoning_tokens\":5}}}}\n\n";
        let no_usage = b"data: {\"type\":\"response.created\",\"response\":{\"id\":\"r\"}}\n\n";
        for (index, bytes, server_error) in [
            (0, usage.as_slice(), false),
            (1, no_usage.as_slice(), false),
            (2, usage.as_slice(), true),
        ] {
            let log = context(storage.clone(), "/v1/responses", "http")
                .start(
                    RequestMetadata {
                        model: Some("gpt-6-astra".into()),
                        ..Default::default()
                    },
                    Instant::now(),
                    index,
                )
                .await
                .unwrap();
            let source = futures::stream::once(async move {
                Ok::<_, std::io::Error>(Bytes::copy_from_slice(bytes))
            })
            .chain(futures::stream::pending());
            let response = reqwest::Response::from(
                http::Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(reqwest::Body::wrap_stream(source))
                    .unwrap(),
            );
            let mut body = log
                .wrap_with(response, |body| {
                    if server_error {
                        Body::from_stream(body.into_data_stream().map(|_| {
                            Err::<Bytes, _>(std::io::Error::other("server-side transform failed"))
                        }))
                    } else {
                        body
                    }
                })
                .into_data_stream();
            let first = body.next().await.unwrap();
            assert_eq!(first.is_err(), server_error);
            drop(body);
            let rows = finalized(&storage, index as usize + 1).await;
            let record = rows.iter().find(|r| r.requested_at_ms == index).unwrap();
            if server_error {
                assert_eq!(record.status, "interrupted");
                assert!(record.error_message.is_some());
            } else {
                assert_eq!(record.status, "client_stopped");
                assert!(record.error_code.is_none());
                assert!(record.error_message.is_none());
                if index == 0 {
                    assert_eq!(record.input_tokens, Some(100));
                    assert_eq!(record.output_tokens, Some(20));
                    assert_eq!(record.billing_status, "priced");
                    assert!(record.cost_nano_usd.unwrap() > 0);
                    storage.finish_usage(record).await.unwrap();
                    assert_eq!(
                        finalized(&storage, 1).await[0].cost_nano_usd,
                        record.cost_nano_usd
                    );
                } else {
                    assert_eq!(record.billing_status, "missing_usage");
                    assert_eq!(record.cost_nano_usd, None);
                }
            }
        }
        storage.close().await;
    }

    #[tokio::test]
    async fn websocket_client_stop_and_cancel_ack_preserve_usage_without_hiding_server_errors() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("ws-stop.sqlite"))
            .await
            .unwrap();
        let context = execution_context(storage.clone(), "/v1/responses", "websocket").await;
        for (index, ending) in ["client", "upstream", "cancel-ack"].into_iter().enumerate() {
            let mut ledger = WsLedger::new(context.clone());
            let log = self::context(storage.clone(), "/v1/responses", "websocket")
                .start(
                    RequestMetadata {
                        model: Some("gpt-6-astra".into()),
                        ..Default::default()
                    },
                    Instant::now(),
                    index as i64,
                )
                .await
                .unwrap();
            ledger.push(Some(log));
            ledger.observe(br#"{"type":"response.created","response":{"id":"r","usage":{"input_tokens":100,"output_tokens":20}}}"#).await.unwrap();
            match ending {
                "client" => { ledger.client_stopped().await.unwrap(); ledger.client_stopped().await.unwrap(); },
                "upstream" => ledger.fail_inflight("upstream_websocket_closed", "上游 WebSocket 在请求完成前关闭").await.unwrap(),
                _ => ledger.observe(br#"{"type":"response.done","response":{"id":"r","status":"cancelled","usage":{"input_tokens":100,"output_tokens":20}}}"#).await.unwrap(),
            }
            let rows = finalized(&storage, index + 1).await;
            let record = rows
                .iter()
                .find(|r| r.requested_at_ms == index as i64)
                .unwrap();
            assert_eq!(record.billing_status, "priced");
            assert!(record.cost_nano_usd.unwrap() > 0);
            if ending == "upstream" {
                assert_eq!(record.status, "failed");
                assert_eq!(
                    record.error_code.as_deref(),
                    Some("upstream_websocket_closed")
                );
                assert!(record.error_message.is_some());
            } else {
                assert_eq!(record.status, "client_stopped");
                assert!(record.error_message.is_none());
            }
        }
        storage.close().await;
    }

    #[tokio::test]
    async fn responses_errors_are_preserved_in_request_details() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("response-errors.sqlite"))
            .await
            .unwrap();
        let cases = [
            (
                "detail",
                "application/json",
                r#"{"detail":"The requested model is not supported."}"#,
                None,
                "The requested model is not supported.",
            ),
            (
                "null-code",
                "application/json",
                r#"{"error":{"code":null,"type":"invalid_request_error","message":"Unsupported parameter: temperature"}}"#,
                Some("invalid_request_error"),
                "Unsupported parameter: temperature",
            ),
            (
                "status-number",
                "application/json",
                r#"{"type":"error","status":400,"error":{"code":"invalid_model","message":"The requested model is not available."}}"#,
                Some("invalid_model"),
                "The requested model is not available.",
            ),
            (
                "sse-error",
                "text/event-stream",
                "data: {\"type\":\"error\",\"code\":\"invalid_prompt\",\"message\":\"Invalid prompt.\"}\n\n",
                Some("invalid_prompt"),
                "Invalid prompt.",
            ),
            (
                "sse-failed",
                "text/event-stream",
                "data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"error\":{\"code\":\"context_length_exceeded\",\"message\":\"Input exceeds the context window.\"}}}\n\ndata: {\"type\":\"response.metadata\",\"headers\":{}}\n\n",
                Some("context_length_exceeded"),
                "Input exceeds the context window.",
            ),
            (
                "code-only",
                "application/json",
                r#"{"error":{"code":"invalid_prompt"}}"#,
                Some("invalid_prompt"),
                "上游返回 HTTP 400",
            ),
            (
                "unstructured",
                "text/html",
                "<html>Bad request</html>",
                Some("upstream_http_error"),
                "上游返回 HTTP 400",
            ),
        ];
        let mut count = 0;
        for (id, content_type, body, code, message) in cases {
            for status_error in [false, true] {
                let id = format!("{id}-{status_error}");
                let mut log = RequestLog::begin(
                    storage.clone(),
                    UsageRecord {
                        id: id.clone(),
                        status: "in_progress".into(),
                        endpoint: "/v1/responses".into(),
                        ..Default::default()
                    },
                    Instant::now(),
                )
                .await
                .unwrap();
                if status_error {
                    log.upstream_failure(&codex2api_upstream::UpstreamError::Status {
                        status: 400,
                        body: body.into(),
                    });
                } else {
                    let chunks = body
                        .as_bytes()
                        .chunks(7)
                        .map(|chunk| Ok::<_, std::io::Error>(Bytes::copy_from_slice(chunk)))
                        .collect::<Vec<_>>();
                    let response = reqwest::Response::from(
                        http::Response::builder()
                            .status(400)
                            .header("content-type", content_type)
                            .body(reqwest::Body::wrap_stream(futures::stream::iter(chunks)))
                            .unwrap(),
                    );
                    let output = axum::body::to_bytes(log.wrap(response), usize::MAX)
                        .await
                        .unwrap();
                    assert_eq!(output.as_ref(), body.as_bytes());
                }
                count += 1;
                let rows = finalized(&storage, count).await;
                let row = rows.iter().find(|row| row.id == id).unwrap();
                assert_eq!(row.status, "failed", "{id}");
                assert_eq!(row.http_status, Some(400), "{id}");
                assert_eq!(row.error_message.as_deref(), Some(message), "{id}");
                assert_eq!(row.error_code.as_deref(), code, "{id}");
            }
        }
        storage.close().await;
    }

    #[tokio::test]
    async fn official_model_headers_and_failure_metadata_survive_http_sse_and_websocket() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("metadata.sqlite"))
            .await
            .unwrap();
        let cases = [
            (
                "http-model",
                200,
                Some("gpt-server-header"),
                "application/json",
                r#"{"model":"requested","status":"completed","usage":{"input_tokens":10,"output_tokens":2}}"#,
            ),
            (
                "sse-model",
                200,
                Some("initial-model"),
                "text/event-stream",
                "data: {\"type\":\"response.metadata\",\"headers\":{\"X-OpenAI-Model\":[\"gpt-routed-model\"],\"X-OAI-Request-ID\":[\"req-sse-event\"]}}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"model\":\"requested\",\"usage\":{\"input_tokens\":12,\"output_tokens\":3}}}\n\n",
            ),
            (
                "sse-failed",
                200,
                None,
                "text/event-stream",
                "data: {\"type\":\"response.failed\",\"response\":{\"model\":\"requested\",\"headers\":{\"OpenAI-Model\":\"gpt-failed-model\"},\"error\":{\"code\":\"context_length_exceeded\",\"message\":\"Your input exceeds the context window.\"},\"usage\":null}}\n\n",
            ),
            (
                "http-failed",
                429,
                None,
                "application/json",
                r#"{"error":{"code":"rate_limit_exceeded","message":"Too many requests. Bearer secret-value sk-private-token"}}"#,
            ),
        ];
        for (i, (id, status, model, content, body)) in cases.into_iter().enumerate() {
            let log = RequestLog::begin(
                storage.clone(),
                UsageRecord {
                    id: id.into(),
                    model: Some("requested".into()),
                    status: "in_progress".into(),
                    endpoint: "/v1/responses".into(),
                    ..Default::default()
                },
                Instant::now(),
            )
            .await
            .unwrap();
            let mut response = http::Response::builder()
                .status(status)
                .header("content-type", content);
            if id == "http-model" {
                response = response.header("x-oai-request-id", format!("req-{id}"));
            } else if id.starts_with("http-") {
                response = response.header("x-request-id", format!("req-{id}"));
            }
            if let Some(model) = model {
                response = response.header("openai-model", model);
            }
            let response =
                reqwest::Response::from(response.body(reqwest::Body::from(body)).unwrap());
            let output = axum::body::to_bytes(log.wrap(response), usize::MAX)
                .await
                .unwrap();
            assert_eq!(output.as_ref(), body.as_bytes());
            finalized(&storage, i + 1).await;
        }
        let rows = finalized(&storage, 4).await;
        let row = |id| rows.iter().find(|r| r.id == id).unwrap();
        assert_eq!(
            row("http-model").upstream_request_id.as_deref(),
            Some("req-http-model")
        );
        assert_eq!(
            row("http-failed").upstream_request_id.as_deref(),
            Some("req-http-failed")
        );
        assert_eq!(
            row("sse-model").upstream_request_id.as_deref(),
            Some("req-sse-event")
        );
        assert!(row("sse-failed").upstream_request_id.is_none());
        assert_eq!(
            row("http-model").actual_model.as_deref(),
            Some("gpt-server-header")
        );
        assert_eq!(
            row("sse-model").actual_model.as_deref(),
            Some("gpt-routed-model")
        );
        assert_eq!(
            row("sse-failed").actual_model.as_deref(),
            Some("gpt-failed-model")
        );
        assert_eq!(row("sse-failed").http_status, Some(200));
        assert_eq!(
            row("sse-failed").error_code.as_deref(),
            Some("context_length_exceeded")
        );
        assert_eq!(
            row("sse-failed").error_message.as_deref(),
            Some("Your input exceeds the context window.")
        );
        assert_eq!(
            row("http-failed").error_code.as_deref(),
            Some("rate_limit_exceeded")
        );
        assert!(
            !row("http-failed")
                .error_message
                .as_deref()
                .unwrap()
                .contains("secret-value")
        );
        assert!(
            !row("http-failed")
                .error_message
                .as_deref()
                .unwrap()
                .contains("sk-private-token")
        );

        let mut ledger =
            WsLedger::new(execution_context(storage.clone(), "/v1/responses", "websocket").await);
        let log = RequestLog::begin(
            storage.clone(),
            UsageRecord {
                id: "websocket-failure".into(),
                status: "in_progress".into(),
                model: Some("requested".into()),
                endpoint: "/v1/responses".into(),
                ..Default::default()
            },
            Instant::now(),
        )
        .await
        .unwrap();
        let mut upgrade_headers = http::HeaderMap::new();
        upgrade_headers.insert(
            "x-request-id",
            http::HeaderValue::from_static("connection-only"),
        );
        upgrade_headers.insert(
            "x-oai-request-id",
            http::HeaderValue::from_static("connection-only-oai"),
        );
        ledger.response_headers(&upgrade_headers);
        ledger.push(Some(log));
        assert!(
            ledger.slots[0]
                .log
                .as_ref()
                .unwrap()
                .record
                .as_ref()
                .unwrap()
                .upstream_request_id
                .is_none()
        );
        ledger.observe(br#"{"type":"response.created","headers":{"openai-model":"wrong-header","x-request-id":"top-request"},"response":{"id":"r1","headers":{"OPENAI-MODEL":"gpt-ws-real","X-Request-ID":"req-ws-generation"}}}"#).await.unwrap();
        ledger.observe(br#"{"type":"response.failed","response":{"id":"r1","model":"requested","error":{"code":"insufficient_quota","message":"Usage limit reached"}}}"#).await.unwrap();
        let rows = storage
            .query_usage(&UsageFilter::default())
            .await
            .unwrap()
            .records;
        let row = rows.iter().find(|r| r.id == "websocket-failure").unwrap();
        assert_eq!(row.actual_model.as_deref(), Some("gpt-ws-real"));
        assert_eq!(row.error_code.as_deref(), Some("insufficient_quota"));
        assert_eq!(row.error_message.as_deref(), Some("Usage limit reached"));
        assert_eq!(
            row.upstream_request_id.as_deref(),
            Some("req-ws-generation")
        );
        storage.close().await;
    }

    #[tokio::test]
    async fn image_results_count_returned_outputs_and_disabled_models_stop_http_and_ws() {
        use codex2api_storage::{ImagePrice, ModelConfig};
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("image-ledger.sqlite"))
            .await
            .unwrap();
        let model = ModelConfig {
            provider_id: "chatgpt".into(),
            model: "image-test".into(),
            kind: "image".into(),
            enabled: true,
            deleted: false,
            revision: 0,
        };
        storage
            .save_model_config(
                &model,
                &[],
                &[
                    ImagePrice {
                        provider_id: "chatgpt".into(),
                        model: model.model.clone(),
                        resolution: "1K".into(),
                        price_nano_usd: 40_000_000,
                    },
                    ImagePrice {
                        provider_id: "chatgpt".into(),
                        model: model.model.clone(),
                        resolution: "2K".into(),
                        price_nano_usd: 80_000_000,
                    },
                ],
                None,
            )
            .await
            .unwrap();
        let mut log = context(storage.clone(), "/v1/images/generations", "http")
            .start(
                RequestMetadata {
                    model: Some(model.model.clone()),
                    image_size: Some("auto".into()),
                    ..Default::default()
                },
                Instant::now(),
                1000,
            )
            .await
            .unwrap();
        let response=br#"{"size":"1024x1024","data":[{"b64_json":"sensitive-image-one"},{"b64_json":"sensitive-image-two","size":"1536x1024"},{"error":{"message":"failed"}},{"b64_json":""}]}"#;
        log.parse(response);
        log.parse(response);
        log.finish("completed");
        let rows = finalized(&storage, 1).await;
        assert_eq!(rows[0].image_count, Some(2));
        assert_eq!(rows[0].cost_nano_usd, Some(120_000_000));
        assert!(
            !rows[0]
                .image_usage_json
                .as_ref()
                .unwrap()
                .contains("sensitive")
        );
        assert!(
            storage
                .set_model_enabled("chatgpt", &model.model, false, 1)
                .await
                .unwrap()
        );
        let denied = execution_context(storage.clone(), "/v1/images/generations", "http")
            .await
            .start(
                RequestMetadata {
                    model: Some(model.model.clone()),
                    ..Default::default()
                },
                Instant::now(),
                2000,
            )
            .await;
        assert!(denied.is_err());
        let ledger = std::sync::Arc::new(tokio::sync::Mutex::new(WsLedger::new(
            execution_context(storage.clone(), "/v1/responses", "websocket").await,
        )));
        assert!(
            ws_start(
                &ledger,
                r#"{"type":"response.create","model":"image-test"}"#
            )
            .await
            .is_err()
        );
        assert_eq!(
            storage
                .query_usage(&UsageFilter::default())
                .await
                .unwrap()
                .total,
            1
        );
    }

    #[tokio::test]
    async fn realtime_vad_generations_record_actual_usage_without_inventing_a_price() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("realtime.sqlite"))
            .await
            .unwrap();
        let mut ledger =
            WsLedger::new(execution_context(storage.clone(), "/v1/realtime", "websocket").await);
        ledger
            .observe(
                br#"{"type":"response.created","response":{"id":"vad","model":"voice-model"}}"#,
            )
            .await
            .unwrap();
        ledger.observe(br#"{"type":"response.done","response":{"id":"vad","status":"completed","usage":{"input_tokens":12,"output_tokens":7,"input_tokens_details":{"cached_tokens":2}}}}"#).await.unwrap();
        let rows = storage.query_usage(&UsageFilter::default()).await.unwrap();
        assert_eq!(rows.total, 1);
        let record = &rows.records[0];
        assert_eq!(record.input_tokens, Some(12));
        assert_eq!(record.output_tokens, Some(7));
        assert_eq!(record.cost_nano_usd, None);
        assert_eq!(record.billing_status, "unsupported");
    }

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
            ("partial", "client_stopped"),
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

    // Accounting unit tests start directly at the ledger boundary. Authorization
    // is exercised separately through the application service and protocol routes.
    struct AccountingFixture {
        storage: Storage,
        endpoint: String,
        transport: &'static str,
    }
    fn context(storage: Storage, endpoint: &str, transport: &'static str) -> AccountingFixture {
        AccountingFixture {
            storage,
            endpoint: endpoint.into(),
            transport,
        }
    }
    impl AccountingFixture {
        async fn start(
            &self,
            metadata: RequestMetadata,
            start: Instant,
            requested_at_ms: i64,
        ) -> crate::Result<RequestLog> {
            RequestLog::begin(
                self.storage.clone(),
                UsageRecord {
                    id: uuid::Uuid::new_v4().to_string(),
                    account_id: "account-1".into(),
                    account_name: "SupplierAccount One".into(),
                    subject_id: "consumer-1".into(),
                    subject_name: "Consumer One".into(),
                    endpoint: self.endpoint.clone(),
                    transport: self.transport.into(),
                    model: metadata.model,
                    reasoning_effort: metadata.reasoning_effort,
                    service_tier: metadata.service_tier,
                    image_size: metadata.image_size,
                    requested_at_ms,
                    status: "in_progress".into(),
                    ..Default::default()
                },
                start,
            )
            .await
        }
    }
    async fn execution_context(
        storage: Storage,
        endpoint: &str,
        transport: &'static str,
    ) -> ExecutionContext {
        let supplier = codex2api_accounts::SupplierAccountStore::open(storage.clone())
            .create_pending()
            .await
            .unwrap()
            .account;
        let consumer = codex2api_storage::VirtualAccount {
            provider_id: "chatgpt".into(),
            id: "consumer-1".into(),
            username: "consumer-1".into(),
            password_hash: "fixture".into(),
            name: "Consumer One".into(),
            email: "c@example.test".into(),
            plan_type: "plus".into(),
            plan_id: "plus".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        storage.save_virtual_account(&consumer).await.unwrap();
        for model in ["warmup", "one", "two", "interrupted", "voice-model"] {
            sqlx::query("INSERT OR IGNORE INTO model_catalog(provider_id,model,kind) VALUES('chatgpt',?,'text')").bind(model).execute(storage.pool()).await.unwrap();
        }
        ExecutionContext::new(
            storage,
            &supplier,
            &consumer.id,
            &consumer.name,
            endpoint,
            transport,
        )
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
        let ledger = tokio::sync::Mutex::new(WsLedger::new(
            execution_context(storage.clone(), "/v1/responses", "websocket").await,
        ));
        ws_start(
            &ledger,
            r#"{"type":"response.create","generate":false,"model":"warmup"}"#,
        )
        .await
        .unwrap();
        ledger
            .lock()
            .await
            .observe(br#"{"type":"response.created","response":{"id":"warmup"}}"#)
            .await
            .unwrap();
        ledger
            .lock()
            .await
            .observe(br#"{"type":"response.completed","response":{"id":"warmup"}}"#)
            .await
            .unwrap();
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
            .observe(br#"{"type":"response.created","response":{"id":"r1","model":"actual-one"}}"#)
            .await
            .unwrap();
        ws_start(
            &ledger,
            r#"{"type":"response.create","model":"two","reasoning":{"effort":"low"}}"#,
        )
        .await
        .unwrap();
        ledger
            .lock()
            .await
            .observe(br#"{"type":"response.created","response":{"id":"r2"}}"#)
            .await
            .unwrap();
        ledger.lock().await.observe(br#"{"type":"response.completed","response":{"id":"r2","model":"actual-two","usage":{"input_tokens":0,"output_tokens":0}}}"#).await.unwrap();
        ledger
            .lock()
            .await
            .observe(br#"{"type":"response.failed","response":{"id":"r1"}}"#)
            .await
            .unwrap();
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

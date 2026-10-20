//! Client request ledger. Never retain prompts, response text, image bytes or bearer keys.

#[cfg(test)]
mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
use crate::execution::ExecutionContext;
#[cfg(test)]
use account_fixture::AccountFixture;
use axum::body::{Body, Bytes};
use codex2api_storage::{Storage, UsageRecord};
use codex2api_upstream::{
    Endpoint, RequestMetadata, ResponseFailure, ResponseLifecycle, ResponseOutcome,
    executor::AttemptLedger,
};
use futures::{Stream, StreamExt};
use serde::Deserialize;
use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

#[cfg(test)]
#[path = "usage_outcome_tests.rs"]
mod outcome_tests;

fn elapsed(start: Instant) -> i64 {
    start.elapsed().as_millis().min(i64::MAX as u128) as i64
}

pub(crate) async fn record_http_failures(
    storage: &Storage,
    access: &codex2api_storage::VirtualAccess,
    endpoint: &str,
    user_agent: Option<String>,
    work: impl std::future::Future<Output = axum::response::Response>,
) -> axum::response::Response {
    let log = codex2api_upstream::RequestFailureLog::new(UsageRecord {
        provider_id: access.provider_id.clone(),
        subject_id: access.virtual_account_id.clone(),
        subject_name: access.name.clone(),
        endpoint: endpoint.into(),
        transport: "http".into(),
        user_agent,
        ..Default::default()
    });
    let response = log.scope(work).await;
    if !log.needs_record(response.status()) {
        return response;
    }
    let (parts, body) = response.into_parts();
    let body = log
        .capture(storage, parts.status, body.into_data_stream())
        .await;
    axum::response::Response::from_parts(parts, Body::from_stream(body))
}

pub(crate) fn billable(endpoint: Endpoint) -> bool {
    !matches!(
        endpoint,
        Endpoint::Models | Endpoint::Usage | Endpoint::InputTokens
    )
}

pub(crate) trait ProtocolObserver: Send + Sync {
    fn response_affinity(&self) -> bool {
        true
    }
    fn observe(&mut self, bytes: &[u8]) -> Vec<serde_json::Value>;
}
pub(crate) struct RequestLog {
    supplier_permit: Option<codex2api_storage::RuntimePermit>,
    consumer_permits: Vec<codex2api_storage::RuntimePermit>,
    timing: Option<codex2api_upstream::ResponseTiming>,
    attempt_reported: bool,
    observer: Option<Box<dyn ProtocolObserver>>,
    record: Option<UsageRecord>,
    storage: Storage,
    start: Instant,
    lifecycle: ResponseLifecycle,
    auth_revision: Option<i64>,
    authoritative_model: bool,
    response_owner: Option<(codex2api_storage::RoutingKey, String)>,
}
impl RequestLog {
    pub(crate) fn set_supplier_permit(&mut self, permit: codex2api_storage::RuntimePermit) {
        self.supplier_permit = Some(permit);
    }
    pub(crate) fn set_consumer_permits(&mut self, permits: Vec<codex2api_storage::RuntimePermit>) {
        self.consumer_permits = permits;
    }
    pub(crate) fn release_supplier_permit(&mut self) {
        self.supplier_permit = None;
    }
    fn report_runtime_result(&mut self, record: &UsageRecord) {
        if !self.attempt_reported {
            self.storage.report_supplier_result(
                &record.account_id,
                self.lifecycle.outcome().map_or_else(
                    || matches!(record.status.as_str(), "completed" | "submitted"),
                    |outcome| outcome.failure().is_none(),
                ),
                self.timing
                    .as_ref()
                    .and_then(|timing| timing.supplier_ttft_ms()),
            );
            self.attempt_reported = true;
        }
        self.supplier_permit = None;
        self.consumer_permits.clear();
    }
    pub(crate) fn set_protocol_observer(&mut self, observer: impl ProtocolObserver + 'static) {
        self.observer = Some(Box::new(observer));
    }
    pub(crate) async fn rebind_supplier(
        &mut self,
        account: &codex2api_storage::SupplierAccount,
    ) -> crate::Result<()> {
        if let Some(record) = &mut self.record {
            self.auth_revision = self.storage.supplier_auth_revision(&account.id).await?;
            if record.account_id == account.id {
                return Ok(());
            }
            record.account_id = account.id.clone();
            record.account_provider_id = Some(account.provider_id.clone());
            record.account_name = account
                .display_name
                .as_deref()
                .or(account.email.as_deref())
                .unwrap_or(&account.id)
                .into();
            self.storage
                .reroute_usage(
                    &record.id,
                    &record.account_id,
                    &record.account_name,
                    &account.provider_id,
                )
                .await?;
        }
        Ok(())
    }

    pub(crate) fn retry_generation(&mut self) {
        self.attempt_reported = false;
        self.timing = None;
        self.lifecycle = ResponseLifecycle::default();
        self.authoritative_model = false;
        if let Some(r) = &mut self.record {
            r.status = "in_progress".into();
            r.http_status = None;
            r.error_code = None;
            r.error_message = None;
            r.public_error_code = None;
            r.public_error_message = None;
            r.upstream_request_id = None;
            r.actual_model = None;
            r.first_token_ms = None;
            r.generation_observed = false;
            r.input_tokens = None;
            r.output_tokens = None;
            r.cached_tokens = None;
            r.cache_write_tokens = None;
            r.reasoning_tokens = None;
            r.image_count = None;
            r.image_usage_json = None;
        }
    }

    pub(crate) async fn begin(
        storage: Storage,
        mut record: UsageRecord,
        start: Instant,
    ) -> crate::Result<Self> {
        let start = if let Some(clock) = codex2api_upstream::RequestClock::current() {
            record.requested_at_ms = clock.requested_at_ms;
            record.client_ip = clock.client_ip();
            clock.started
        } else {
            start
        };
        let auth_revision = storage.supplier_auth_revision(&record.account_id).await?;
        storage.insert_usage(&record).await?;
        codex2api_upstream::RequestFailureLog::recorded();
        Ok(Self {
            supplier_permit: None,
            consumer_permits: Vec::new(),
            attempt_reported: false,
            timing: None,
            observer: None,
            storage,
            record: Some(record),
            start,
            lifecycle: ResponseLifecycle::default(),
            auth_revision,
            authoritative_model: false,
            response_owner: None,
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
struct ResponseMetadata<'a> {
    id: Option<String>,
    model: Option<String>,
    headers: Option<serde_json::Value>,
    size: Option<String>,
    usage: Option<Tokens>,
    status: Option<String>,
    #[serde(borrow)]
    data: Option<Vec<ImageOutput<'a>>>,
}
#[derive(Default, Deserialize)]
struct ImageOutput<'a> {
    b64_json: Option<&'a str>,
    url: Option<&'a str>,
    size: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    error: Option<serde::de::IgnoredAny>,
}
#[derive(Default, Deserialize)]
struct Event<'a> {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(borrow)]
    response: Option<ResponseMetadata<'a>>,
    response_id: Option<String>,
    model: Option<String>,
    headers: Option<serde_json::Value>,
    size: Option<String>,
    usage: Option<Tokens>,
    // Error envelopes use a numeric HTTP status; response events use a string.
    status: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
    #[serde(borrow)]
    data: Option<Vec<ImageOutput<'a>>>,
}

impl Event<'_> {
    fn generation_message(&self) -> bool {
        self.kind
            .as_deref()
            .is_some_and(|kind| kind.starts_with("response.") || kind == "error")
    }
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

pub(crate) fn error_text(value: &str, limit: usize) -> String {
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
    if record.public_error_code.is_none() {
        let (code, message) = match record.status.as_str() {
            "interrupted" => (
                "request_interrupted",
                "The request was interrupted before it completed.",
            ),
            "incomplete" => (
                "response_incomplete",
                "The response ended before it was complete.",
            ),
            _ => (
                "upstream_error",
                "The upstream service could not complete this request.",
            ),
        };
        record.public_error_code = Some(code.into());
        record.public_error_message = Some(message.into());
    }
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
        self.lifecycle
            .fail(ResponseFailure::new(None, Some(code), Some(message)));
        self.apply_outcome();
    }
    pub fn upstream_failure(&mut self, error: &codex2api_upstream::UpstreamError) {
        self.auth_revision = error.auth_revision().or(self.auth_revision);
        if let codex2api_upstream::UpstreamError::Status {
            status,
            headers,
            body,
            ..
        } = error
        {
            self.http_status(*status);
            self.response_headers(headers);
            let mut parser = BodyParser::default();
            parser.feed(body.as_bytes(), false, self);
            parser.end(false, self);
        }
        let mut failure = error.failure();
        if !matches!(error, codex2api_upstream::UpstreamError::Status { .. }) {
            failure.message = Some(codex2api_upstream::request_failure::error_diagnostic(error));
        }
        if let Some(record) = &mut self.record
            && record.http_status.is_none()
            && record.transport == "http"
        {
            record.http_status = failure.status.map(i64::from);
        }
        self.lifecycle.fail(failure);
        self.apply_outcome();
        self.finish("failed");
    }
    fn capture_timing(&mut self) {
        if let Some(record) = &mut self.record {
            record.first_token_ms = self
                .timing
                .as_ref()
                .and_then(|timing| timing.first_token_ms(self.start));
            record.generation_observed |= record.first_token_ms.is_some();
        }
    }
    pub fn http_status(&mut self, status: u16) {
        if let Some(record) = &mut self.record {
            record.http_status = Some(i64::from(status));
        }
    }
    fn observe(&mut self, event: &Event<'_>, value: &serde_json::Value) {
        if self.lifecycle.outcome().is_some() {
            return;
        }
        let Some(record) = &mut self.record else {
            return;
        };
        record.generation_observed |= codex2api_upstream::scheduler::generation_observed(value);
        if let Some(id) = event
            .response
            .as_ref()
            .and_then(|r| r.id.as_deref())
            .or_else(|| value.get("id").and_then(serde_json::Value::as_str))
            .filter(|id| id.len() <= 512)
        {
            if self
                .observer
                .as_ref()
                .is_none_or(|observer| observer.response_affinity())
                && (event.kind.as_deref() == Some("response.completed")
                    || (event.kind.is_none() && value.get("output").is_some()))
            {
                let key = crate::pool_scheduler::previous_key(
                    &record.subject_id,
                    &record.provider_id,
                    id,
                );
                self.response_owner = Some((key, record.account_id.clone()));
            }
        }
        let response = event.response.as_ref();
        if let Some(id) = response
            .and_then(|r| r.headers.as_ref())
            .and_then(reported_request_id)
            .or_else(|| event.headers.as_ref().and_then(reported_request_id))
        {
            record.upstream_request_id = Some(id);
        }
        // Speed and billing use the client's request tier, never the response tier.
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
        }
        if !search
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
        let http_status = record.http_status.and_then(|s| u16::try_from(s).ok());
        if self.lifecycle.observe(value, http_status) {
            self.apply_outcome();
        }
    }
    fn apply_outcome(&mut self) {
        self.capture_timing();
        let (Some(outcome), Some(record)) = (self.lifecycle.outcome(), &mut self.record) else {
            return;
        };
        record.status = outcome.record_status().into();
        if let Some(failure) = outcome.failure() {
            record.error_code = failure.code.as_deref().map(|v| error_text(v, 128));
            record.error_message = failure.message.as_deref().map(|v| error_text(v, 512));
            // The client receives the supplier's own failure with its message redacted.
            record.public_error_code = Some(
                failure
                    .code
                    .clone()
                    .unwrap_or_else(|| "upstream_error".into()),
            );
            record.public_error_message = Some(
                failure
                    .message
                    .as_deref()
                    .map(codex2api_upstream::convert::sanitize::text)
                    .unwrap_or_default(),
            );
        }
    }
    /// Keep a failed supplier attempt the pool recovers from (`retry` or `failover`);
    /// administrators see it, the client only sees the final outcome.
    pub(crate) async fn recovered(
        &self,
        error: &codex2api_upstream::UpstreamError,
        recovery: &str,
    ) -> crate::Result<()> {
        if let Some(record) = &self.record {
            self.storage
                .record_usage_attempt(&codex2api_upstream::request_failure::failed_attempt(
                    record,
                    error,
                    recovery,
                    record.requested_at_ms,
                ))
                .await?;
        }
        Ok(())
    }
    fn parse(&mut self, bytes: &[u8]) {
        if let Some(observer) = &mut self.observer {
            let events = observer.observe(bytes);
            for value in events {
                if let Ok(event) = Event::deserialize(&value) {
                    self.observe(&event, &value);
                }
            }
            return;
        }
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
            self.lifecycle
                .observe(&serde_json::json!({"type":"response.completed"}), None);
            return;
        }
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes)
            && let Ok(event) = Event::deserialize(&value)
        {
            self.observe(&event, &value);
        }
    }
    fn take_finished(&mut self, status: &str) -> Option<UsageRecord> {
        self.capture_timing();
        let mut record = self.record.take()?;
        if record.status == "in_progress" {
            record.status = status.into();
        }
        complete_failure_reason(&mut record);
        record.total_ms = Some(elapsed(self.start));
        self.report_runtime_result(&record);
        Some(record)
    }
    pub fn finish(&mut self, status: &str) {
        let Some(record) = self.take_finished(status) else {
            return;
        };
        let storage = self.storage.clone();
        tokio::spawn(async move {
            if let Err(error) = storage.finish_usage(&record).await {
                tracing::error!(%error,record_id=%record.id,"failed to finalize usage record");
            }
        });
    }
    pub(crate) async fn settle_failure(
        &mut self,
        error: &codex2api_upstream::executor::ExecutorError,
    ) -> crate::Result<()> {
        use codex2api_upstream::executor::ExecutorError as E;
        let failure = match error {
            E::Client { status, body, .. } | E::Stopped { status, body, .. } => {
                ResponseFailure::from_error(Some(*status), body)
            }
            E::Convert(error) => {
                ResponseFailure::from_error(Some(error.status.as_u16()), &error.body)
            }
            E::Upstream(error) => error.failure(),
            E::ConsumerInvalid => ResponseFailure::new(
                Some(401),
                Some("invalid_token"),
                Some("Consumer authorization was revoked"),
            ),
            E::ContextRequired => ResponseFailure::new(
                Some(409),
                Some("supplier_context_required"),
                Some("The previous response cannot be continued"),
            ),
            E::NoSupplier {
                model_not_found: true,
            } => ResponseFailure::new(
                Some(404),
                Some("model_not_found"),
                Some("No supplier is configured for this model"),
            ),
            E::NoSupplier { .. } | E::Busy => ResponseFailure::new(
                Some(503),
                Some("no_available_supplier"),
                Some("No supplier is available"),
            ),
            E::Storage(_) => {
                ResponseFailure::new(Some(500), Some("internal_error"), Some("Storage failed"))
            }
        };
        if let Some(status) = failure.status
            && self
                .record
                .as_ref()
                .is_some_and(|r| r.http_status.is_none())
        {
            self.http_status(status);
        }
        self.lifecycle.fail(failure);
        self.apply_outcome();
        if matches!(error, E::Stopped { status, .. } if codex2api_upstream::supply::switch_status(*status))
            && let Some(record) = &mut self.record
        {
            record.public_error_code = Some("no_available_supplier".into());
            record.public_error_message =
                Some("No supplier is available for this request. Please retry later.".into());
        }
        if let Some(record) = self.take_finished("failed") {
            self.storage.finish_usage(&record).await?;
        }
        Ok(())
    }
    pub fn wrap(self, response: reqwest::Response) -> Body {
        self.wrap_with(response, |body| body)
    }
    async fn persist_response_owner(&mut self) -> codex2api_storage::Result<()> {
        if let Some((key, supplier)) = self.response_owner.take() {
            self.storage.remember_response_owner(key, supplier).await?;
        }
        Ok(())
    }
    pub fn wrap_with(
        mut self,
        response: reqwest::Response,
        transform: impl FnOnce(Body) -> Body,
    ) -> Body {
        self.timing = response
            .extensions()
            .get::<codex2api_upstream::ResponseTiming>()
            .cloned();
        self.auth_revision = response
            .extensions()
            .get::<codex2api_upstream::SupplierAuthRevision>()
            .map(|r| r.0)
            .or(self.auth_revision);
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
        let mut stream = ObservedStream {
            inner: Box::pin(response.bytes_stream()),
            log: self,
            sse,
            success,
            parser: BodyParser::default(),
            expected_bytes,
            received_bytes: 0,
            ended: false,
            idle: Box::pin(tokio::time::sleep(
                codex2api_upstream::DEFAULT_STREAM_IDLE_TIMEOUT,
            )),
        };
        if expected_bytes == Some(0) {
            stream.finish_body();
        }
        let stream = futures::stream::try_unfold(Box::new(stream), |mut stream| async move {
            let chunk = stream.next().await;
            // The client may immediately continue this response on another connection.
            stream
                .log
                .persist_response_owner()
                .await
                .map_err(std::io::Error::other)?;
            chunk
                .transpose()
                .map_err(std::io::Error::other)
                .map(|chunk| chunk.map(|bytes| (bytes, stream)))
        });
        Body::from_stream(ClientBody {
            inner: Some(transform(Body::from_stream(stream)).into_data_stream()),
            ended: false,
        })
    }
}
impl Drop for RequestLog {
    fn drop(&mut self) {
        self.finish("interrupted");
    }
}

/// Unbilled client endpoints run without a ledger; billed ones attribute every
/// attempt and keep recovered failures for the administrator.
pub(crate) struct Ledger<'a>(pub(crate) &'a mut Option<RequestLog>);
impl AttemptLedger for Ledger<'_> {
    async fn rebind(
        &mut self,
        account: &codex2api_storage::SupplierAccount,
    ) -> std::result::Result<(), codex2api_upstream::UpstreamError> {
        if let Some(log) = self.0 {
            log.rebind_supplier(account)
                .await
                .map_err(|error| codex2api_upstream::UpstreamError::Internal(error.to_string()))?;
        }
        Ok(())
    }
    async fn attempt_failed(
        &mut self,
        error: &codex2api_upstream::UpstreamError,
        recovery: &str,
    ) -> std::result::Result<(), codex2api_upstream::UpstreamError> {
        if let Some(log) = self.0 {
            log.recovered(error, recovery)
                .await
                .map_err(|error| codex2api_upstream::UpstreamError::Internal(error.to_string()))?;
            log.attempt_reported = true;
            if recovery == "failover" {
                log.release_supplier_permit();
            }
        }
        Ok(())
    }
    fn measured_failure(
        &mut self,
        _: codex2api_upstream::supply::Wire,
        failure: &codex2api_upstream::supply::UpstreamFailure,
    ) {
        if let Some(log) = self.0 {
            if let Some(status) = failure.status {
                log.http_status(status);
            }
            log.response_headers(&failure.headers);
            let bytes = failure.body.to_string();
            let mut parser = BodyParser::default();
            parser.feed(bytes.as_bytes(), false, log);
            parser.end(false, log);
        }
    }
    fn retry_generation(&mut self) {
        if let Some(log) = self.0 {
            log.retry_generation();
        }
    }
}

// The client owns delivery, while the service owns an accepted upstream request.
// Keep the parser and account-local transforms alive after delivery is cancelled;
// only a real upstream/transform error or terminal response ends settlement.
struct ClientBody {
    inner: Option<axum::body::BodyDataStream>,
    ended: bool,
}
impl Stream for ClientBody {
    type Item = Result<Bytes, axum::Error>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let Some(inner) = &mut this.inner else {
            return Poll::Ready(None);
        };
        let result = Pin::new(inner).poll_next(cx);
        if matches!(result, Poll::Ready(None | Some(Err(_)))) {
            this.ended = true;
        }
        result
    }
}
impl Drop for ClientBody {
    fn drop(&mut self) {
        if !self.ended
            && !std::thread::panicking()
            && let Some(mut stream) = self.inner.take()
        {
            tokio::spawn(async move {
                while let Some(chunk) = stream.next().await {
                    if chunk.is_err() {
                        break;
                    }
                }
            });
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
                if codex2api_upstream::ResponseFormat::from_prefix(&self.data)
                    == Some(codex2api_upstream::ResponseFormat::EventStream)
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
    ended: bool,
    idle: Pin<Box<tokio::time::Sleep>>,
}
impl ObservedStream {
    fn finish_body(&mut self) {
        if self.log.record.is_none() {
            return;
        }
        self.parser.end(self.sse, &mut self.log);
        if self.log.lifecycle.outcome().is_none() {
            if !self.success {
                let status = self
                    .log
                    .record
                    .as_ref()
                    .and_then(|r| r.http_status)
                    .and_then(|s| u16::try_from(s).ok());
                self.log
                    .lifecycle
                    .fail(ResponseFailure::new(status, None, None));
                self.log.apply_outcome();
            } else if self.sse || self.parser.detected_sse {
                self.log
                    .failure("upstream_incomplete", "上游连接在生成终止事件之前结束");
            }
        }
        self.log.finish("completed");
    }
    fn terminate_stream(&mut self, status: u16, code: &str, message: &str) -> Bytes {
        self.ended = true;
        self.log.lifecycle.fail(ResponseFailure::new(
            Some(status),
            Some(code),
            Some(message),
        ));
        self.log.apply_outcome();
        self.log.finish("failed");
        // This event describes the broken generation. It cannot change the
        // already-established HTTP status or reject the supplier's credentials.
        let value = serde_json::json!({"type":"response.failed","status":status,
            "response":{"status":"failed","error":{"code":code,"message":message}}});
        Bytes::from(format!("\n\nevent: response.failed\ndata: {value}\n\n"))
    }
}
impl Stream for ObservedStream {
    type Item = Result<Bytes, reqwest::Error>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::future::Future;
        let this = self.get_mut();
        if this.ended {
            return Poll::Ready(None);
        }
        match this.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                this.idle.as_mut().reset(
                    tokio::time::Instant::now() + codex2api_upstream::DEFAULT_STREAM_IDLE_TIMEOUT,
                );
                this.parser.feed(&bytes, this.sse, &mut this.log);
                if this.log.lifecycle.outcome().is_some() {
                    this.log.finish("completed");
                }
                this.received_bytes = this.received_bytes.saturating_add(bytes.len() as u64);
                if this.expected_bytes == Some(this.received_bytes) && !this.sse {
                    this.finish_body();
                }
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(error))) => {
                if this.log.lifecycle.outcome().is_some() {
                    this.ended = true;
                    this.finish_body();
                    return Poll::Ready(None);
                }
                let error = error.without_url();
                let code = if error.is_timeout() {
                    "upstream_timeout"
                } else {
                    "upstream_stream_error"
                };
                let mut detail = error.to_string();
                let mut source = std::error::Error::source(&error);
                while let Some(cause) = source {
                    detail.push_str(": ");
                    detail.push_str(&cause.to_string());
                    source = cause.source();
                }
                let message = error_text(&detail, 512);
                if this.sse {
                    Poll::Ready(Some(Ok(this.terminate_stream(
                        if error.is_timeout() { 504 } else { 502 },
                        code,
                        &message,
                    ))))
                } else {
                    this.ended = true;
                    this.log.failure(code, &message);
                    this.log.finish("failed");
                    Poll::Ready(Some(Err(error)))
                }
            }
            Poll::Ready(None) => {
                this.parser.end(this.sse, &mut this.log);
                if this.sse && this.success && this.log.lifecycle.outcome().is_none() {
                    return Poll::Ready(Some(Ok(this.terminate_stream(
                        502,
                        "upstream_incomplete",
                        "上游连接在生成终止事件之前结束",
                    ))));
                }
                this.ended = true;
                this.finish_body();
                Poll::Ready(None)
            }
            Poll::Pending if this.sse && this.idle.as_mut().poll(cx).is_ready() => {
                if this.log.lifecycle.outcome().is_some() {
                    this.ended = true;
                    return Poll::Ready(None);
                }
                Poll::Ready(Some(Ok(this.terminate_stream(
                    504,
                    "upstream_timeout",
                    "等待上游响应流超时",
                ))))
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
    slots: VecDeque<WsSlot>,
    initial_headers: Option<http::HeaderMap>,
}
impl WsLedger {
    pub(crate) fn set_timing(&mut self, timing: Option<codex2api_upstream::ResponseTiming>) {
        if let Some(log) = self.slots.front_mut().and_then(|slot| slot.log.as_mut()) {
            log.timing = timing;
        }
    }
    pub(crate) async fn recovered(
        &mut self,
        error: &codex2api_upstream::UpstreamError,
        recovery: &str,
    ) -> crate::Result<()> {
        for slot in &mut self.slots {
            if let Some(log) = &mut slot.log {
                log.recovered(error, recovery).await?;
                log.attempt_reported = true;
            }
        }
        Ok(())
    }

    pub(crate) fn release_supplier_permits(&mut self) {
        for slot in &mut self.slots {
            if let Some(log) = &mut slot.log {
                log.release_supplier_permit();
            }
        }
    }
    pub(crate) async fn retry_supplier(
        &mut self,
        context: ExecutionContext,
        account: &codex2api_storage::SupplierAccount,
    ) -> crate::Result<()> {
        self.context = context;
        self.initial_headers = None;
        for slot in &mut self.slots {
            slot.response_id = None;
            if let Some(log) = &mut slot.log {
                if let Some(permit) = self.context.take_supplier_permit() {
                    log.set_supplier_permit(permit);
                }
                log.retry_generation();
                log.rebind_supplier(account).await?;
            }
        }
        Ok(())
    }
    pub fn new(context: ExecutionContext) -> Self {
        Self {
            context,
            slots: VecDeque::new(),
            initial_headers: None,
        }
    }
    pub fn response_headers(&mut self, headers: &http::HeaderMap) {
        let mut initial = headers.clone();
        // The upgrade request ID belongs to the connection, not a generation.
        for name in ["x-request-id", "x-oai-request-id", "x-openai-request-id"] {
            initial.remove(name);
        }
        if let Some(log) = self.slots.front_mut().and_then(|slot| slot.log.as_mut()) {
            log.response_headers(&initial);
        } else {
            self.initial_headers = Some(initial);
        }
    }
    #[cfg(test)]
    pub fn has_pending(&self) -> bool {
        !self.slots.is_empty()
    }
    #[cfg(test)]
    pub async fn fail_inflight(&mut self, code: &str, message: &str) -> crate::Result<()> {
        self.fail_pending(ResponseFailure::new(None, Some(code), Some(message)))
            .await
    }
    pub async fn fail_pending(&mut self, failure: ResponseFailure) -> crate::Result<()> {
        for mut slot in self.slots.drain(..) {
            if let Some(log) = &mut slot.log {
                log.lifecycle.fail(failure.clone());
                log.apply_outcome();
                if let Some(mut record) = log.record.take() {
                    complete_failure_reason(&mut record);
                    record.total_ms = Some(elapsed(log.start));
                    log.storage.finish_usage(&record).await?;
                    log.report_runtime_result(&record);
                }
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
    #[cfg(test)]
    pub async fn observe(&mut self, bytes: &[u8]) -> crate::Result<()> {
        self.observe_message(bytes).await.map(|_| ())
    }
    pub(crate) async fn observe_message(
        &mut self,
        bytes: &[u8],
    ) -> crate::Result<Option<ResponseOutcome>> {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            return Ok(None);
        };
        self.observe_value(&value).await
    }
    pub(crate) async fn observe_value(
        &mut self,
        value: &serde_json::Value,
    ) -> crate::Result<Option<ResponseOutcome>> {
        let Ok(event) = Event::deserialize(value) else {
            return Ok(None);
        };
        if !event.generation_message() {
            return Ok(None);
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
        let Some(index) = index else {
            if response_id.is_none()
                && let Some(ResponseOutcome::Failed(failure)) =
                    ResponseOutcome::from_value(&value, None)
            {
                self.fail_pending(failure.clone()).await?;
                return Ok(Some(ResponseOutcome::Failed(failure)));
            }
            return Ok(None);
        };
        let slot = &mut self.slots[index];
        if slot.response_id.is_none() {
            slot.response_id = response_id.map(str::to_string);
        }
        if let Some(log) = &mut slot.log {
            log.observe(&event, &value);
            log.persist_response_owner().await?;
        }
        let outcome = ResponseOutcome::from_value(&value, None);
        if outcome.is_some()
            && let Some(mut log) = self.slots.remove(index).and_then(|s| s.log)
            && let Some(mut record) = log.record.take()
        {
            complete_failure_reason(&mut record);
            record.total_ms = Some(elapsed(log.start));
            log.storage.finish_usage(&record).await?;
            log.report_runtime_result(&record);
        }
        Ok(outcome)
    }
}

pub(crate) async fn ws_start(
    ledger: &tokio::sync::Mutex<WsLedger>,
    value: &serde_json::Value,
    admission: Option<crate::execution::Admission>,
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
    let value = Create::deserialize(value)
        .map_err(|_| crate::OAuthError::bad_request("Invalid Responses frame fields."))?;
    if value.kind.as_deref() != Some("response.create") {
        return Ok(());
    }
    let start = Instant::now();
    let now = chrono::Utc::now().timestamp_millis();
    let context = ledger.lock().await.context.clone();
    let log = if value.generate == Some(false) {
        drop(context.take_supplier_permit());
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
        let metadata = RequestMetadata {
            model: value.model,
            reasoning_effort: value
                .reasoning
                .and_then(|r| r.effort)
                .map(|s| s.chars().take(64).collect()),
            service_tier: value.service_tier.map(|s| s.chars().take(64).collect()),
            image_size: value.size.map(|s| s.chars().take(64).collect()),
            generate: value.generate,
            ..Default::default()
        };
        Some(match admission {
            Some(admission) => context.start_admitted(admission, start, now).await?,
            None => context.start(metadata, start, now).await?,
        })
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
    async fn request_service_tier_controls_speed_and_billing_across_transports() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("service-tier-pg"))
            .await
            .unwrap();
        let ws_context = execution_context(storage.clone(), "/v1/responses", "websocket").await;
        let mut count = 0;
        for transport in ["json", "sse", "websocket"] {
            for (requested, reported, billing_tier, cost) in [
                (Some("priority"), "default", Some("fast"), Some(28_200_000)),
                (Some("fast"), "flex", Some("fast"), Some(28_200_000)),
                (Some("flex"), "priority", Some("flex"), Some(7_050_000)),
                (None, "priority", Some("standard"), Some(14_100_000)),
                (
                    Some("default"),
                    "priority",
                    Some("standard"),
                    Some(14_100_000),
                ),
                (Some("custom-speed"), "default", None, None),
            ] {
                count += 1;
                let request = serde_json::json!({
                    "model":"gpt-6-astra", "service_tier":requested
                });
                let metadata = codex2api_upstream::request_metadata(
                    &serde_json::to_vec(&request).unwrap(),
                    &http::HeaderMap::new(),
                )
                .unwrap();
                let mut log = context(storage.clone(), "/v1/responses", transport)
                    .start(metadata, Instant::now(), count as i64)
                    .await
                    .unwrap();
                let id = log.record.as_ref().unwrap().id.clone();
                let response = serde_json::json!({
                    "id":format!("response-{count}"), "model":"gpt-6-astra",
                    "status":"completed", "service_tier":reported,
                    "usage":{"input_tokens":1000,"output_tokens":100,
                        "input_tokens_details":{"cached_tokens":100}}
                });
                let created = serde_json::json!({
                    "type":"response.created", "service_tier":reported,
                    "response":{"id":response["id"],"service_tier":reported}
                });
                let completed = serde_json::json!({
                    "type":"response.completed", "response":response
                });
                if transport == "websocket" {
                    let mut ledger = WsLedger::new(ws_context.clone());
                    ledger.push(Some(log));
                    ledger
                        .observe(created.to_string().as_bytes())
                        .await
                        .unwrap();
                    ledger
                        .observe(completed.to_string().as_bytes())
                        .await
                        .unwrap();
                } else {
                    let sse = transport == "sse";
                    let body = if sse {
                        format!("data: {created}\n\ndata: {completed}\n\n")
                    } else {
                        response.to_string()
                    };
                    let mut parser = BodyParser::default();
                    log.http_status(200);
                    for chunk in body.as_bytes().chunks(17) {
                        parser.feed(chunk, sse, &mut log);
                    }
                    parser.end(sse, &mut log);
                    log.finish("completed");
                }
                let rows = finalized(&storage, count).await;
                let row = rows.iter().find(|row| row.id == id).unwrap();
                assert_eq!(row.service_tier.as_deref(), requested, "{transport}");
                assert_eq!(row.billing_tier.as_deref(), billing_tier, "{transport}");
                assert_eq!(row.cost_nano_usd, cost, "{transport}");
                assert_eq!(
                    row.billing_status,
                    if cost.is_some() { "priced" } else { "unpriced" }
                );
                assert_eq!(row.status, "completed");
            }
        }
        storage.close().await;
    }

    #[tokio::test]
    async fn supplier_retry_keeps_the_original_request_service_tier() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage =
            codex2api_storage::test_support::open(dir.path().join("service-tier-retry-pg"))
                .await
                .unwrap();
        let mut log = context(storage.clone(), "/v1/responses", "sse")
            .start(
                RequestMetadata {
                    model: Some("gpt-6-astra".into()),
                    service_tier: Some("priority".into()),
                    ..Default::default()
                },
                Instant::now(),
                1,
            )
            .await
            .unwrap();
        log.parse(br#"{"type":"response.created","response":{"service_tier":"default"}}"#);
        log.retry_generation();
        log.parse(br#"{"type":"response.completed","response":{"usage":{"input_tokens":1000,"output_tokens":100,"input_tokens_details":{"cached_tokens":100}}}}"#);
        log.finish("completed");
        let rows = finalized(&storage, 1).await;
        assert_eq!(rows[0].service_tier.as_deref(), Some("priority"));
        assert_eq!(rows[0].billing_tier.as_deref(), Some("fast"));
        assert_eq!(rows[0].cost_nano_usd, Some(28_200_000));
        storage.close().await;
    }

    #[tokio::test]
    async fn http_disconnect_drains_unpolled_and_partial_bodies_and_settles_final_usage() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("disconnect-pg"))
            .await
            .unwrap();
        for (index, read_first, failed) in [(0, false, false), (1, true, false), (2, true, true)] {
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
            let (finish, ready) = tokio::sync::oneshot::channel::<()>();
            let initial = Bytes::from_static(
                b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial output\"}\n\n",
            );
            let source = futures::stream::once(async move { Ok::<_, std::io::Error>(initial) })
                .chain(futures::stream::once(async move {
                    ready.await.unwrap();
                    let value = if failed {
                        serde_json::json!({"type":"response.failed","response":{"error":{"code":"rate_limit_exceeded","message":"retry later"},"usage":{"input_tokens":700,"output_tokens":30}}})
                    } else {
                        serde_json::json!({"type":"response.completed","response":{"usage":{"input_tokens":700,"output_tokens":30,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0}}}})
                    };
                    Ok(Bytes::from(format!("data: {value}\n\n")))
                }));
            let response = reqwest::Response::from(
                http::Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(reqwest::Body::wrap_stream(source))
                    .unwrap(),
            );
            let mut body = log.wrap(response).into_data_stream();
            if read_first {
                body.next().await.unwrap().unwrap();
            }
            drop(body);
            let pending = storage.query_usage(&UsageFilter::default()).await.unwrap();
            assert_eq!(
                pending
                    .records
                    .iter()
                    .find(|r| r.requested_at_ms == index)
                    .unwrap()
                    .status,
                "in_progress"
            );
            finish.send(()).unwrap();
            let rows = finalized(&storage, index as usize + 1).await;
            let record = rows.iter().find(|r| r.requested_at_ms == index).unwrap();
            assert_eq!(record.status, if failed { "failed" } else { "completed" });
            assert_eq!(
                (record.input_tokens, record.output_tokens),
                (Some(700), Some(30))
            );
            if failed {
                assert_eq!(record.error_code.as_deref(), Some("rate_limit_exceeded"));
            } else {
                assert_eq!(record.billing_status, "priced");
                assert!(record.cost_nano_usd.unwrap() > 0);
                assert!(record.error_code.is_none());
                storage.finish_usage(record).await.unwrap();
                assert_eq!(
                    finalized(&storage, index as usize + 1)
                        .await
                        .iter()
                        .find(|r| r.id == record.id)
                        .unwrap()
                        .cost_nano_usd,
                    record.cost_nano_usd
                );
            }
        }
        storage.close().await;
    }

    #[tokio::test]
    async fn server_transform_failure_is_not_treated_as_client_disconnect() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("transform-pg"))
            .await
            .unwrap();
        let log = context(storage.clone(), "/v1/responses", "http")
            .start(RequestMetadata::default(), Instant::now(), 0)
            .await
            .unwrap();
        let source = futures::stream::once(async {
            Ok::<_, std::io::Error>(Bytes::from_static(
                b"data: {\"type\":\"response.created\"}\n\n",
            ))
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
                Body::from_stream(
                    body.into_data_stream()
                        .map(|_| Err::<Bytes, _>(std::io::Error::other("transform failed"))),
                )
            })
            .into_data_stream();
        assert!(body.next().await.unwrap().is_err());
        drop(body);
        assert_eq!(finalized(&storage, 1).await[0].status, "interrupted");
    }

    #[tokio::test]
    async fn websocket_client_stop_and_cancel_ack_preserve_usage_without_hiding_server_errors() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("ws-stop-pg"))
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
                "client" => { assert!(ledger.has_pending()); ledger.observe(br#"{"type":"response.completed","response":{"id":"r","usage":{"input_tokens":100,"output_tokens":20}}}"#).await.unwrap(); },
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
                assert_eq!(
                    record.status,
                    if ending == "client" {
                        "completed"
                    } else {
                        "client_stopped"
                    }
                );
                assert!(record.error_message.is_none());
            }
        }
        storage.close().await;
    }

    #[tokio::test]
    async fn responses_errors_are_preserved_in_request_details() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("response-errors-pg"))
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
                        auth_revision: None,
                        status: 400,
                        body: body.into(),
                        headers: http::HeaderMap::new(),
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
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("metadata-pg"))
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
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("image-ledger-pg"))
            .await
            .unwrap();
        let model = ModelConfig {
            platform: "openai".into(),
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
                        model: model.model.clone(),
                        resolution: "1K".into(),
                        price_nano_usd: 40_000_000,
                    },
                    ImagePrice {
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
        let response=br#"{"size":"1024x1024","data":[{"b64_json":"sensitive-image-one"},{"b64_json":"sensitive-image-two","size":"1536x1024"},{"error":{"message":"failed"}},{"b64_json":""}],"usage":{"input_tokens":123,"output_tokens":456,"input_tokens_details":{"cached_tokens":23}}}"#;
        log.parse(response);
        log.parse(response);
        log.finish("completed");
        let rows = finalized(&storage, 1).await;
        assert_eq!(rows[0].image_count, Some(2));
        assert_eq!(rows[0].cost_nano_usd, Some(120_000_000));
        assert_eq!(rows[0].input_tokens, Some(123));
        assert_eq!(rows[0].output_tokens, Some(456));
        assert_eq!(rows[0].cached_tokens, Some(23));
        assert!(
            !rows[0]
                .image_usage_json
                .as_ref()
                .unwrap()
                .contains("sensitive")
        );
        assert!(
            storage
                .set_model_enabled(&model.model, false, 1)
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
                &serde_json::from_str(r#"{"type":"response.create","model":"image-test"}"#)
                    .unwrap(),
                None,
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
    async fn length_tracking_preserves_partial_disconnects_errors_and_chunked_completion() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("usage-pg"))
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
            ("partial", "completed"),
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
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("usage-pg"))
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
        assert!(rows[0].first_token_ms.is_none());
        assert!(rows[0].total_ms.is_some());
        assert!(rows[0].input_tokens.is_none());
        storage.close().await;
    }

    #[tokio::test]
    async fn final_sse_usage_is_recorded_without_content_type_and_across_split_prefixes() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("usage-pg"))
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

    #[tokio::test]
    async fn bookkeeping_does_not_invent_ttft_without_upstream_observation() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("timing-pg"))
            .await
            .unwrap();
        let mut log = context(storage.clone(), "/v1/responses", "http")
            .start(RequestMetadata::default(), Instant::now(), 1000)
            .await
            .unwrap();
        log.parse(br#"{"type":"response.created","response":{"id":"r"}}"#);
        log.parse(br#"{"type":"response.output_text.delta","delta":"text"}"#);
        log.parse(br#"{"type":"response.completed","response":{"id":"r"}}"#);
        log.finish("completed");
        let rows = finalized(&storage, 1).await;
        assert_eq!(rows[0].first_token_ms, None);
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
        let supplier = codex2api_suppliers::SupplierAccountStore::open(storage.clone())
            .create_pending()
            .await
            .unwrap()
            .account;
        let consumer = codex2api_storage::PlatformAccount {
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
        storage.save_account_fixture(&consumer).await.unwrap();
        let mut granted = Vec::new();
        for model in ["warmup", "one", "two", "interrupted"] {
            sqlx::query("INSERT INTO model_catalog(model,platform,kind) VALUES($1,'chatgpt','text') ON CONFLICT DO NOTHING").bind(model).execute(storage.pool()).await.unwrap();
            storage
                .add_supplier_model(&supplier.id, model)
                .await
                .unwrap();
            granted.push(codex2api_common::ModelRule::granted(model));
        }
        // A plan grants only the models in its explicit list.
        let mut plan = storage
            .virtual_plan(&consumer.plan_id)
            .await
            .unwrap()
            .unwrap();
        plan.config["models"] = serde_json::json!(granted);
        assert!(
            storage
                .save_virtual_plan(&plan, Some(plan.revision))
                .await
                .unwrap()
        );
        ExecutionContext::new(
            storage,
            &supplier,
            &consumer.id,
            &consumer.name,
            endpoint,
            transport,
        )
    }
    #[tokio::test]
    async fn successful_task_submission_preserves_status_and_supplier_health() {
        let directory = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(directory.path().join("submitted-pg"))
            .await
            .unwrap();
        let context = context(storage.clone(), "/backend-api/codex/tasks", "http");
        for _ in 0..4 {
            let mut log = context
                .start(RequestMetadata::default(), Instant::now(), 1)
                .await
                .unwrap();
            log.http_status(202);
            log.finish("submitted");
        }
        let records = finalized(&storage, 4).await;
        assert!(records.iter().all(|record| record.status == "submitted"));
        assert_eq!(storage.supplier_metrics("account-1").error_rate, 0.0);
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
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("usage-pg"))
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
        let timing = codex2api_upstream::ResponseTiming::default();
        let mut stream = log
            .wrap(
                timing
                    .clone()
                    .wrap(response, codex2api_upstream::TokenProtocol::Responses),
            )
            .into_data_stream();
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
        assert_eq!(timing.supplier_ttft_ms(), None);
        let delta = Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"first\"}\n\n",
        );
        tx.send(Ok(delta.clone())).await.unwrap();
        assert_eq!(stream.next().await.unwrap().unwrap(), delta);
        assert!(timing.supplier_ttft_ms().is_some());
        let completed=b"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"model\":\"actual\",\"service_tier\":\"default\",\"usage\":{\"input_tokens\":100,\"output_tokens\":20,\"input_tokens_details\":{\"cached_tokens\":40,\"cache_write_tokens\":5},\"output_tokens_details\":{\"reasoning_tokens\":10}}}}\r\n\r\ndata: [DONE]\r\n\r\n";
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
        assert!(row.first_token_ms.is_some());
        assert!(row.total_ms >= row.first_token_ms);
        server.abort();
        storage.close().await;
    }

    #[tokio::test]
    async fn websocket_interrupt_acknowledgements_preserve_terminal_usage_in_either_order() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("interrupts-pg"))
            .await
            .unwrap();
        let ledger = tokio::sync::Mutex::new(WsLedger::new(
            execution_context(storage.clone(), "/v1/responses", "websocket").await,
        ));
        for (index, (ack, first)) in [("accepted", true), ("accepted", false), ("failed", true)]
            .into_iter()
            .enumerate()
        {
            let id = format!("interrupt-{index}");
            ws_start(
                &ledger,
                &serde_json::json!({"type":"response.create","model":"one"}),
                None,
            )
            .await
            .unwrap();
            ledger
                .lock()
                .await
                .observe_value(&serde_json::json!({"type":"response.created","response":{"id":id}}))
                .await
                .unwrap();
            ws_start(
                &ledger,
                &serde_json::json!({"type":"response.interrupt","response_id":id}),
                None,
            )
            .await
            .unwrap();
            let ack =
                serde_json::json!({"type":format!("response.interrupt.{ack}"),"response_id":id});
            let end = serde_json::json!({"type":"response.incomplete","response":{"id":id,"status":"incomplete","incomplete_details":{"reason":"interrupted"},"usage":{"input_tokens":100,"output_tokens":20}}});
            for event in if first { [&ack, &end] } else { [&end, &ack] } {
                ledger.lock().await.observe_value(event).await.unwrap();
            }
            assert!(!ledger.lock().await.has_pending());
            // A duplicated final event and subsequent transport teardown must not settle twice.
            ledger.lock().await.observe_value(&end).await.unwrap();
        }
        drop(ledger);
        let records = finalized(&storage, 3).await;
        for record in records {
            assert_eq!(
                (record.input_tokens, record.output_tokens),
                (Some(100), Some(20))
            );
            assert_eq!(record.status, "incomplete");
            assert_eq!(record.error_code.as_deref(), Some("interrupted"));
        }
        storage.close().await;
    }

    #[tokio::test]
    async fn websocket_records_each_generation_skips_warmups_and_keeps_missing_usage_unknown() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("usage-pg"))
            .await
            .unwrap();
        let ledger = tokio::sync::Mutex::new(WsLedger::new(
            execution_context(storage.clone(), "/v1/responses", "websocket").await,
        ));
        ws_start(
            &ledger,
            &serde_json::from_str(
                r#"{"type":"response.create","generate":false,"model":"warmup"}"#,
            )
            .unwrap(),
            None,
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
            &serde_json::from_str(r#"{"type":"response.create","model":"one","reasoning":{"effort":"high"},"service_tier":"priority"}"#).unwrap(), None,
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
            &serde_json::from_str(
                r#"{"type":"response.create","model":"two","reasoning":{"effort":"low"}}"#,
            )
            .unwrap(),
            None,
        )
        .await
        .unwrap();
        // Official 0.160.0 steering interrupts a response without creating a new generation.
        ws_start(
            &ledger,
            &serde_json::from_str(r#"{"type":"response.interrupt","response_id":"r1","mode":"discard_partial_items"}"#).unwrap(), None,
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
            .observe(
                br#"{"type":"response.failed","response":{"id":"r1","service_tier":"default"}}"#,
            )
            .await
            .unwrap();
        ws_start(
            &ledger,
            &serde_json::from_str(r#"{"type":"response.create","model":"interrupted"}"#).unwrap(),
            None,
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
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("usage-pg"))
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

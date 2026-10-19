//! Metadata-only usage observation and exactly-once API wallet settlement.
use crate::{ApiError, Protocol, Result};
use codex2api_storage::{Storage, UsageRecord};
use codex2api_upstream::{ResponseFailure, ResponseLifecycle, ResponseOutcome};
use serde_json::{Value, json};
use std::time::Instant;

pub(crate) struct ApiLog {
    storage: Storage,
    pub(crate) record: UsageRecord,
    pub(crate) lifecycle: ResponseLifecycle,
    pub(crate) auth_revision: Option<i64>,
    started: Instant,
    settled: bool,
    authoritative_model: bool,
    timing: Option<codex2api_upstream::ResponseTiming>,
}
impl ApiLog {
    pub(crate) fn new(storage: Storage, record: UsageRecord, started: Instant) -> Self {
        codex2api_upstream::RequestFailureLog::recorded();
        Self {
            storage,
            record,
            lifecycle: ResponseLifecycle::default(),
            auth_revision: None,
            started,
            settled: false,
            authoritative_model: false,
            timing: None,
        }
    }
    pub(crate) fn set_timing(&mut self, timing: Option<codex2api_upstream::ResponseTiming>) {
        self.timing = timing;
    }
    pub(crate) fn headers(&mut self, status: http::StatusCode, headers: &http::HeaderMap) {
        self.record.http_status = Some(i64::from(status.as_u16()));
        self.record.actual_model = None;
        self.authoritative_model = false;
        for name in ["openai-model", "x-openai-model"] {
            if let Some(model) = headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .filter(|v| v.len() <= 256)
            {
                self.record.actual_model = Some(model.into());
                self.authoritative_model = true;
                break;
            }
        }
        self.record.upstream_request_id = ["x-request-id", "x-oai-request-id"]
            .into_iter()
            .find_map(|name| {
                headers
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .filter(|v| v.len() <= 256)
                    .map(str::to_owned)
            });
    }
    /// `upstream` is the supplier protocol whose events are being measured.
    pub(crate) fn observe(&mut self, value: &Value, upstream: Protocol) -> bool {
        if self.lifecycle.outcome().is_some() {
            return false;
        }
        self.measure(value, upstream);
        if self.lifecycle.observe(value, None) {
            return true;
        }
        let completed = match upstream {
            Protocol::ImageGeneration | Protocol::ImageEdit => {
                value["data"].is_array() && self.record.image_count.is_some_and(|count| count > 0)
            }
            Protocol::Compact => value["output"].is_array(),
            Protocol::InputTokens => value["input_tokens"].as_u64().is_some(),
            Protocol::Search => {
                value["output"].is_string()
                    && value
                        .get("encrypted_output")
                        .is_none_or(|v| v.is_null() || v.is_string())
                    && value
                        .get("results")
                        .is_none_or(|v| v.is_null() || v.is_array())
            }
            Protocol::Responses | Protocol::ChatCompletions | Protocol::Messages => false,
        };
        completed
            && self
                .lifecycle
                .observe(&json!({"type":"response.completed"}), None)
    }
    /// Preserve measurements in physical HTTP failures without trusting a success-shaped body.
    pub(crate) fn measure(&mut self, value: &Value, upstream: Protocol) {
        self.record.generation_observed |=
            codex2api_upstream::scheduler::generation_observed(value);
        let response = value.get("response").unwrap_or(value);
        if !self.authoritative_model {
            if let Some(model) = response["model"]
                .as_str()
                .filter(|model| model.len() <= 256)
            {
                self.record.actual_model = Some(model.into());
            }
        }
        let usage = &response["usage"];
        self.tokens(
            &usage["input_tokens"],
            &usage["output_tokens"],
            &usage["input_tokens_details"],
            &usage["output_tokens_details"],
        );
        if upstream.image() {
            crate::images::observe_usage(&mut self.record, response);
        }
    }
    fn tokens(
        &mut self,
        input: &Value,
        output: &Value,
        input_details: &Value,
        output_details: &Value,
    ) {
        fn measurement(value: &Value) -> Option<i64> {
            value.as_i64().filter(|value| *value >= 0)
        }
        if let Some(value) = measurement(input) {
            self.record.input_tokens = Some(value);
        }
        if let Some(value) = measurement(output) {
            self.record.output_tokens = Some(value);
        }
        if let Some(value) = measurement(&input_details["cached_tokens"]) {
            self.record.cached_tokens = Some(value);
        }
        if let Some(value) = measurement(&input_details["cache_write_tokens"]) {
            self.record.cache_write_tokens = Some(value);
        }
        if let Some(value) = measurement(&output_details["reasoning_tokens"]) {
            self.record.reasoning_tokens = Some(value);
        }
    }
    pub(crate) fn fail(&mut self, error: &ApiError) {
        if self.record.transport == "http" && self.record.http_status.is_none() {
            self.record.http_status = Some(i64::from(error.status.as_u16()));
        }
        // A malformed/unsupported response must fail before it is released, even
        // if its body claimed completion. A settled generation is immutable.
        if !self.settled && self.successful() {
            self.lifecycle = ResponseLifecycle::default();
        }
        let mut failure = ResponseFailure::from_error(Some(error.status.as_u16()), &error.body);
        if let Some(message) = &error.diagnostic {
            failure.message = Some(message.clone());
        }
        self.lifecycle.fail(failure);
    }
    pub(crate) fn fail_upstream(&mut self, error: &codex2api_upstream::UpstreamError) {
        let mut failure = error.failure();
        if !matches!(error, codex2api_upstream::UpstreamError::Status { .. }) {
            failure.message = Some(codex2api_upstream::request_failure::error_diagnostic(error));
        }
        if self.record.transport == "http" && self.record.http_status.is_none() {
            self.record.http_status = failure.status.map(i64::from);
        }
        self.lifecycle.fail(failure);
    }
    pub(crate) async fn settle(&mut self) -> Result<()> {
        if self.settled {
            return Ok(());
        }
        if self.lifecycle.outcome().is_none() {
            self.fail(&ApiError::upstream_incomplete());
        }
        self.finalize_record();
        let outcome = self.lifecycle.outcome().expect("terminal outcome");
        self.storage.finish_usage(&self.record).await?;
        self.settled = true;
        let client_backoff = outcome.failure().is_some_and(|failure| {
            matches!(
                failure.kind,
                codex2api_upstream::FailureKind::RateLimit
                    | codex2api_upstream::FailureKind::LimitUnknown
            )
        });
        if !self.record.account_id.is_empty() && !client_backoff {
            self.storage.report_supplier_result(
                &self.record.account_id,
                matches!(outcome, ResponseOutcome::Completed),
                self.timing
                    .as_ref()
                    .and_then(|timing| timing.supplier_ttft_ms()),
            );
        }
        Ok(())
    }
    fn finalize_record(&mut self) {
        self.record.first_token_ms = self
            .timing
            .as_ref()
            .and_then(|timing| timing.first_token_ms(self.started));
        self.record.generation_observed |= self.record.first_token_ms.is_some();
        let outcome = self.lifecycle.outcome();
        self.record.status = outcome
            .map_or("interrupted", ResponseOutcome::record_status)
            .into();
        self.record.total_ms =
            Some(self.started.elapsed().as_millis().min(i64::MAX as u128) as i64);
        if let Some(failure) = outcome.and_then(ResponseOutcome::failure) {
            self.record.error_code = failure.code.clone();
            self.record.error_message = failure
                .message
                .as_deref()
                .map(|message| message.chars().take(512).collect());
        } else if outcome.is_none() {
            self.record.error_code = Some("request_interrupted".into());
            self.record.error_message = Some("请求处理已中断".into());
        }
    }
    pub(crate) fn successful(&self) -> bool {
        matches!(
            self.lifecycle.outcome(),
            Some(ResponseOutcome::Completed | ResponseOutcome::Incomplete(_))
        )
    }
}
impl Drop for ApiLog {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        self.finalize_record();
        let record = self.record.clone();
        let storage = self.storage.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
            if let Err(error)=storage.finish_usage(&record).await {tracing::error!(%error,request_id=%record.id,"Could not settle interrupted API request");}
        });
        }
    }
}

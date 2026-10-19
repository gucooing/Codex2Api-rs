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
    /// When the current supplier attempt started.
    attempt_started: i64,
    /// The failure the caller received, once one was sent or decided.
    public: Option<(String, String)>,
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
            attempt_started: chrono::Utc::now().timestamp_millis(),
            public: None,
            started,
            settled: false,
            authoritative_model: false,
            timing: None,
        }
    }
    /// Attribute the current attempt to the selected supplier account.
    pub(crate) fn supplier(&mut self, account: &codex2api_storage::SupplierAccount) {
        self.record.account_id = account.id.clone();
        self.record.account_name = account
            .display_name
            .clone()
            .unwrap_or_else(|| account.email.clone().unwrap_or_default());
        self.record.account_provider_id = Some(account.provider_id.clone());
    }
    /// Provider of the supplier serving the current attempt. It selects the adapter's
    /// normalization and failure rules; `record.provider_id` stays the platform the
    /// request is identified with.
    pub(crate) fn supplier_provider(&self) -> &str {
        self.record
            .account_provider_id
            .as_deref()
            .unwrap_or_default()
    }
    /// Start another upstream attempt on the current supplier.
    pub(crate) fn begin_attempt(&mut self) {
        self.record.http_status = None;
        self.attempt_started = chrono::Utc::now().timestamp_millis();
    }
    /// Keep a failed supplier attempt that the gateway recovers from by refreshing its
    /// credential (`refresh`) or by selecting another supplier (`failover`). The
    /// administrator sees it; the caller only sees the request's final outcome.
    pub(crate) async fn recovered(
        &self,
        error: &codex2api_upstream::UpstreamError,
        recovery: &str,
    ) -> Result<()> {
        self.storage
            .record_usage_attempt(&codex2api_upstream::request_failure::failed_attempt(
                &self.record,
                error,
                recovery,
                self.attempt_started,
            ))
            .await?;
        Ok(())
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
            // A supplier's own failure event reaches the caller in its public view.
            if self.public.is_none()
                && let Some(failure) = self.lifecycle.outcome().and_then(ResponseOutcome::failure)
            {
                self.public = Some(public_view(
                    &codex2api_upstream::public_output::PublicFailure::new(failure),
                ));
            }
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
            let status = error.cause.as_ref().and_then(|cause| cause.status);
            self.record.http_status = Some(i64::from(status.unwrap_or(error.status.as_u16())));
        }
        // A malformed/unsupported response must fail before it is released, even
        // if its body claimed completion. A settled generation is immutable.
        if !self.settled && self.successful() {
            self.lifecycle = ResponseLifecycle::default();
        }
        // The ledger keeps the real failure; the caller's view is what `error` sends.
        let mut failure = error.cause.clone().unwrap_or_else(|| {
            ResponseFailure::from_error(Some(error.status.as_u16()), &error.body)
        });
        if let Some(message) = &error.diagnostic {
            failure.message = Some(message.clone());
        }
        if self.lifecycle.outcome().is_none() || self.public.is_none() {
            self.public = Some((
                error.body["error"]["code"]
                    .as_str()
                    .unwrap_or("request_failed")
                    .into(),
                error.body["error"]["message"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
            ));
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
        let public = if let Some(failure) = outcome.and_then(ResponseOutcome::failure) {
            self.record.error_code = failure.code.clone();
            self.record.error_message = failure
                .message
                .as_deref()
                .map(|message| codex2api_upstream::request_failure::diagnostic_text(message, 512));
            Some(self.public.clone().unwrap_or_else(|| {
                public_view(&codex2api_upstream::public_output::PublicFailure::new(
                    failure,
                ))
            }))
        } else if outcome.is_none() {
            self.record.error_code = Some("request_interrupted".into());
            self.record.error_message = Some("请求处理已中断".into());
            Some((
                "request_interrupted".into(),
                "The request was interrupted before it completed.".into(),
            ))
        } else {
            None
        };
        (
            self.record.public_error_code,
            self.record.public_error_message,
        ) = match public {
            Some((code, message)) => (Some(code), Some(message.chars().take(512).collect())),
            None => (None, None),
        };
    }
    pub(crate) fn successful(&self) -> bool {
        matches!(
            self.lifecycle.outcome(),
            Some(ResponseOutcome::Completed | ResponseOutcome::Incomplete(_))
        )
    }
}
fn public_view(public: &codex2api_upstream::public_output::PublicFailure) -> (String, String) {
    (public.code.clone(), public.message.clone())
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

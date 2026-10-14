//! Metadata-only usage observation and exactly-once API wallet settlement.
use crate::{GatewayError, OutputMode, Result};
use codex2api_storage::{Storage, UsageRecord};
use codex2api_upstream::{ResponseFailure, ResponseLifecycle, ResponseOutcome};
use serde_json::{Value, json};
use std::time::Instant;

pub(crate) struct ApiLog {
    storage: Storage,
    pub(crate) record: UsageRecord,
    pub(crate) lifecycle: ResponseLifecycle,
    pub(crate) chat_finished: bool,
    pub(crate) auth_revision: Option<i64>,
    chat_choices: std::collections::BTreeMap<u64, bool>,
    started: Instant,
    settled: bool,
    authoritative_model: bool,
}
impl ApiLog {
    pub(crate) fn new(storage: Storage, record: UsageRecord, started: Instant) -> Self {
        Self {
            storage,
            record,
            lifecycle: ResponseLifecycle::default(),
            chat_finished: false,
            auth_revision: None,
            chat_choices: Default::default(),
            started,
            settled: false,
            authoritative_model: false,
        }
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
    pub(crate) fn observe(&mut self, value: &Value, mode: OutputMode) -> bool {
        if self.lifecycle.outcome().is_some() {
            return false;
        }
        self.measure(value, mode);
        if self.lifecycle.observe(value, None) {
            return true;
        }
        let completed = match mode {
            OutputMode::ChatCompletions => {
                for choice in value["choices"].as_array().into_iter().flatten() {
                    let finished = !choice["finish_reason"].is_null();
                    self.chat_finished |= finished;
                    let index = choice["index"].as_u64().unwrap_or(0);
                    *self.chat_choices.entry(index).or_default() |= finished;
                }
                value["object"] == "chat.completion"
                    && !self.chat_choices.is_empty()
                    && self.chat_choices.values().all(|finished| *finished)
            }
            OutputMode::Images => {
                value["data"].is_array() && self.record.image_count.is_some_and(|count| count > 0)
            }
            OutputMode::Compact => value["output"].is_array(),
            OutputMode::InputTokens => value["input_tokens"].as_u64().is_some(),
            OutputMode::Search => {
                value["output"].is_string()
                    && value
                        .get("encrypted_output")
                        .is_none_or(|v| v.is_null() || v.is_string())
                    && value
                        .get("results")
                        .is_none_or(|v| v.is_null() || v.is_array())
            }
            OutputMode::Responses | OutputMode::ChatFromResponses => false,
        };
        completed
            && self
                .lifecycle
                .observe(&json!({"type":"response.completed"}), None)
    }
    /// Preserve measurements in physical HTTP failures without trusting a success-shaped body.
    pub(crate) fn measure(&mut self, value: &Value, mode: OutputMode) {
        if self.record.first_byte_ms.is_none() {
            self.record.first_byte_ms =
                Some(self.started.elapsed().as_millis().min(i64::MAX as u128) as i64);
        }
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
        if mode == OutputMode::ChatCompletions {
            self.tokens(
                &usage["prompt_tokens"],
                &usage["completion_tokens"],
                &usage["prompt_tokens_details"],
                &usage["completion_tokens_details"],
            );
        } else if mode == OutputMode::Images {
            crate::images::observe_usage(&mut self.record, response);
        } else {
            self.tokens(
                &usage["input_tokens"],
                &usage["output_tokens"],
                &usage["input_tokens_details"],
                &usage["output_tokens_details"],
            );
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
    pub(crate) fn done(&mut self, mode: OutputMode) {
        if mode == OutputMode::ChatCompletions
            && !self.chat_choices.is_empty()
            && self.chat_choices.values().all(|finished| *finished)
        {
            self.lifecycle
                .observe(&json!({"type":"response.completed"}), None);
        }
    }
    pub(crate) fn fail(&mut self, error: &GatewayError) {
        // A malformed/unsupported response must fail before it is released, even
        // if its body claimed completion. A settled generation is immutable.
        if !self.settled && self.successful() {
            self.lifecycle = ResponseLifecycle::default();
        }
        self.lifecycle.fail(ResponseFailure::from_error(
            Some(error.status.as_u16()),
            &error.body,
        ));
    }
    pub(crate) fn fail_upstream(&mut self, error: &codex2api_upstream::UpstreamError) {
        self.lifecycle.fail(error.failure());
    }
    pub(crate) async fn settle(&mut self) -> Result<()> {
        if self.settled {
            return Ok(());
        }
        if self.lifecycle.outcome().is_none() {
            self.fail(&GatewayError::upstream_incomplete());
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
                self.record.first_byte_ms.map(|value| value as f64),
            );
        }
        Ok(())
    }
    fn finalize_record(&mut self) {
        let outcome = self.lifecycle.outcome();
        self.record.status = outcome
            .map_or("interrupted", ResponseOutcome::record_status)
            .into();
        self.record.total_ms =
            Some(self.started.elapsed().as_millis().min(i64::MAX as u128) as i64);
        if let Some(failure) = outcome.and_then(ResponseOutcome::failure) {
            self.record.failure_kind = Some(failure.kind.as_str().into());
            self.record.failure_status = failure.status.map(i64::from);
            self.record.error_code = failure.code.clone();
            self.record.error_message = failure
                .message
                .as_deref()
                .map(|message| message.chars().take(512).collect());
        } else if outcome.is_none() {
            self.record.failure_kind = Some("transport".into());
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

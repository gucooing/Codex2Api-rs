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
            started,
            settled: false,
            authoritative_model: false,
        }
    }
    pub(crate) fn headers(&mut self, status: http::StatusCode, headers: &http::HeaderMap) {
        self.record.http_status = Some(i64::from(status.as_u16()));
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
        if mode == OutputMode::ChatCompletions {
            self.chat_finished |= value["choices"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| !row["finish_reason"].is_null()));
            if value["object"] == "chat.completion"
                && self.chat_finished
                && value.get("error").is_none_or(Value::is_null)
            {
                return self
                    .lifecycle
                    .observe(&json!({"type":"response.completed"}), None);
            }
        } else if mode == OutputMode::Images {
            if value.get("error").is_none_or(Value::is_null)
                && value["data"].is_array()
                && self.record.image_count.is_some_and(|count| count > 0)
            {
                return self
                    .lifecycle
                    .observe(&json!({"type":"response.completed"}), None);
            }
        }
        self.lifecycle.observe(value, None)
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
        if mode == OutputMode::ChatCompletions && self.chat_finished {
            self.lifecycle
                .observe(&json!({"type":"response.completed"}), None);
        }
    }
    pub(crate) fn fail(&mut self, error: &GatewayError) {
        self.lifecycle.fail(ResponseFailure::from_error(
            Some(error.status.as_u16()),
            &error.body,
        ));
    }
    pub(crate) async fn settle(&mut self) -> Result<()> {
        if self.settled {
            return Ok(());
        }
        if self.lifecycle.outcome().is_none() {
            self.fail(&GatewayError::upstream_incomplete());
        }
        let outcome = self.lifecycle.outcome().expect("terminal outcome");
        self.record.status = outcome.record_status().into();
        self.record.total_ms =
            Some(self.started.elapsed().as_millis().min(i64::MAX as u128) as i64);
        if let Some(failure) = outcome.failure() {
            self.record.failure_kind = Some(failure.kind.as_str().into());
            self.record.failure_status = failure.status.map(i64::from);
            self.record.error_code = failure.code.clone();
            // Never persist provider prose; it can include identities or content.
            self.record.error_message =
                Some("The upstream service could not complete this request.".into());
        }
        self.storage.finish_api_request(&self.record).await?;
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
        let mut record = self.record.clone();
        record.status = self
            .lifecycle
            .outcome()
            .map_or("failed", ResponseOutcome::record_status)
            .into();
        if self.lifecycle.outcome().is_none() {
            record.failure_kind = Some("interrupted".into());
        }
        record.total_ms = Some(self.started.elapsed().as_millis().min(i64::MAX as u128) as i64);
        let storage = self.storage.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
            if let Err(error)=storage.finish_api_request(&record).await {tracing::error!(%error,request_id=%record.id,"Could not settle interrupted API request");}
        });
        }
    }
}

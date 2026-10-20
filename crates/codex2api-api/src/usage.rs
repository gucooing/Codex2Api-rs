//! Metadata-only usage observation and exactly-once API wallet settlement. The
//! ledger reads every wire through the conversion matrix's observation, never a
//! wire's events directly.
use crate::{ApiError, Result};
use codex2api_storage::{Storage, UsageRecord};
use codex2api_upstream::{
    ResponseFailure, ResponseLifecycle, ResponseOutcome,
    convert::{Observation, Outcome},
    executor::AttemptLedger,
    supply::Operation,
};
use serde_json::{Value, json};
use std::time::Instant;

pub(crate) struct ApiLog {
    storage: Storage,
    endpoint_log: codex2api_upstream::RequestEndpoint,
    pub(crate) record: UsageRecord,
    pub(crate) lifecycle: ResponseLifecycle,
    pub(crate) auth_revision: Option<i64>,
    /// When the current supplier attempt started.
    attempt_started: i64,
    /// The failure the caller received, once one was sent or decided.
    public: Option<(String, String)>,
    started: Instant,
    settled: bool,
    attempt_reported: bool,
    authoritative_model: bool,
    timing: Option<codex2api_upstream::ResponseTiming>,
}
impl ApiLog {
    pub(crate) fn new(storage: Storage, record: UsageRecord, started: Instant) -> Self {
        codex2api_upstream::RequestFailureLog::recorded();
        Self {
            endpoint_log: codex2api_upstream::RequestEndpoint::new(
                storage.clone(),
                record.id.clone(),
            ),
            storage,
            record,
            lifecycle: ResponseLifecycle::default(),
            auth_revision: None,
            attempt_started: chrono::Utc::now().timestamp_millis(),
            public: None,
            started,
            settled: false,
            attempt_reported: false,
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
    /// Start another upstream attempt on the current supplier.
    pub(crate) fn begin_attempt(&mut self) {
        self.attempt_reported = false;
        self.record.http_status = None;
        self.attempt_started = chrono::Utc::now().timestamp_millis();
    }
    /// Keep a failed supplier attempt that the gateway recovers from by retrying the
    /// same supplier (`retry`) or by selecting another one (`failover`). The
    /// administrator sees it; the caller only sees the request's final outcome.
    pub(crate) async fn recovered(
        &self,
        error: &codex2api_upstream::UpstreamError,
        recovery: &str,
    ) -> Result<()> {
        let mut record = self.record.clone();
        record.upstream_endpoint = self.endpoint_log.latest().or(record.upstream_endpoint);
        self.storage
            .record_usage_attempt(&codex2api_upstream::request_failure::failed_attempt(
                &record,
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
    /// Record what one wire event reports. Returns whether the generation ended.
    pub(crate) fn observe(&mut self, observation: &Observation) -> bool {
        if self.lifecycle.outcome().is_some() {
            return false;
        }
        self.measure(observation);
        let outcome = match &observation.outcome {
            Some(Outcome::Completed) => ResponseOutcome::Completed,
            Some(Outcome::Incomplete { reason }) => ResponseOutcome::Incomplete(ResponseFailure {
                kind: codex2api_upstream::FailureKind::Incomplete,
                status: None,
                code: reason.clone(),
                message: Some("上游生成未完成".into()),
            }),
            Some(Outcome::Failed { status, body }) => {
                ResponseOutcome::Failed(ResponseFailure::from_error(*status, body))
            }
            Some(Outcome::Cancelled) => ResponseOutcome::Cancelled,
            None => return false,
        };
        self.lifecycle.fail_with(outcome);
        true
    }
    /// A unary reply of a channel operation completed when it has its result.
    pub(crate) fn observe_operation(&mut self, operation: Operation, value: &Value) -> bool {
        if self.lifecycle.outcome().is_some() {
            return false;
        }
        if operation.image() {
            crate::images::observe_usage(&mut self.record, value);
            if let Some(model) = value["model"].as_str().filter(|m| m.len() <= 256) {
                self.record.actual_model = Some(model.into());
            }
        }
        let completed = match operation {
            Operation::ImageGeneration | Operation::ImageEdit => {
                value["data"].is_array() && self.record.image_count.is_some_and(|count| count > 0)
            }
            Operation::Compact => value["output"].is_array(),
            Operation::InputTokens => value["input_tokens"].as_u64().is_some(),
            Operation::Search => {
                value["output"].is_string()
                    && value
                        .get("encrypted_output")
                        .is_none_or(|v| v.is_null() || v.is_string())
                    && value
                        .get("results")
                        .is_none_or(|v| v.is_null() || v.is_array())
            }
            Operation::Generate => false,
        };
        completed
            && self
                .lifecycle
                .observe(&json!({"type":"response.completed"}), None)
    }
    /// Preserve measurements, including those in a physical HTTP failure.
    pub(crate) fn measure(&mut self, observation: &Observation) {
        self.record.generation_observed |= observation.observed;
        if !self.authoritative_model
            && let Some(model) = &observation.model
        {
            self.record.actual_model = Some(model.clone());
        }
        if let Some(usage) = &observation.usage {
            if let Some(value) = usage.input {
                self.record.input_tokens = Some(value);
            }
            if let Some(value) = usage.output {
                self.record.output_tokens = Some(value);
            }
            if let Some(value) = usage.cached {
                self.record.cached_tokens = Some(value);
            }
            if let Some(value) = usage.cache_write {
                self.record.cache_write_tokens = Some(value);
            }
            if let Some(value) = usage.reasoning {
                self.record.reasoning_tokens = Some(value);
            }
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
        if !self.record.account_id.is_empty() && !self.attempt_reported {
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
        if let Some(endpoint) = self.endpoint_log.latest() {
            self.record.upstream_endpoint = Some(endpoint);
        }
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
                (
                    failure
                        .code
                        .clone()
                        .unwrap_or_else(|| "upstream_error".into()),
                    failure
                        .message
                        .as_deref()
                        .map(codex2api_upstream::convert::sanitize::text)
                        .unwrap_or_default(),
                )
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

impl AttemptLedger for ApiLog {
    fn endpoint_log(&self) -> Option<codex2api_upstream::RequestEndpoint> {
        Some(self.endpoint_log.clone())
    }
    async fn rebind(
        &mut self,
        account: &codex2api_storage::SupplierAccount,
    ) -> std::result::Result<(), codex2api_upstream::UpstreamError> {
        if self.record.account_id == account.id {
            return Ok(());
        }
        // A new supplier has not received a request yet. Keep the preceding
        // destination on its failed attempt, never on this supplier's record.
        self.endpoint_log.clear();
        self.record.upstream_endpoint = None;
        self.supplier(account);
        self.storage
            .reroute_usage(
                &self.record.id,
                &self.record.account_id,
                &self.record.account_name,
                &account.provider_id,
                None,
            )
            .await?;
        Ok(())
    }
    async fn attempt_failed(
        &mut self,
        error: &codex2api_upstream::UpstreamError,
        recovery: &str,
    ) -> std::result::Result<(), codex2api_upstream::UpstreamError> {
        self.attempt_reported = true;
        self.recovered(error, recovery)
            .await
            .map_err(|error| codex2api_upstream::UpstreamError::Internal(error.to_string()))
    }
    fn measured_failure(
        &mut self,
        wire: codex2api_upstream::supply::Wire,
        failure: &codex2api_upstream::supply::UpstreamFailure,
    ) {
        if let Some(status) = failure
            .status
            .and_then(|s| http::StatusCode::from_u16(s).ok())
        {
            self.headers(status, &failure.headers);
        }
        self.measure(&codex2api_upstream::convert::observe(wire, &failure.body));
    }
    fn retry_generation(&mut self) {
        self.lifecycle = ResponseLifecycle::default();
        self.timing = None;
        self.authoritative_model = false;
        self.record.http_status = None;
        self.record.actual_model = None;
        self.record.upstream_request_id = None;
        self.record.generation_observed = false;
        self.record.input_tokens = None;
        self.record.output_tokens = None;
        self.record.cached_tokens = None;
        self.record.cache_write_tokens = None;
        self.record.reasoning_tokens = None;
        self.record.image_count = None;
        self.record.image_usage_json = None;
        self.begin_attempt();
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

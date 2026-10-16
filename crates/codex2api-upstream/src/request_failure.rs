//! Record authenticated requests that fail before an execution ledger exists.
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Instant,
};

use bytes::Bytes;
use codex2api_storage::{Storage, UsageRecord};
use futures::{Stream, StreamExt};

tokio::task_local! {
    static CURRENT: RequestFailureLog;
}

#[derive(Clone)]
pub struct RequestFailureLog {
    state: Arc<Mutex<State>>,
    started: Instant,
}

struct State {
    record: UsageRecord,
    recorded: bool,
    diagnostic: Option<String>,
}

impl RequestFailureLog {
    pub fn new(mut record: UsageRecord) -> Self {
        let clock = crate::RequestClock::current().unwrap_or_default();
        record.id = uuid::Uuid::new_v4().to_string();
        record.requested_at_ms = clock.requested_at_ms;
        Self {
            state: Arc::new(Mutex::new(State {
                record,
                recorded: false,
                diagnostic: None,
            })),
            started: clock.started,
        }
    }

    pub async fn scope<F: Future>(&self, work: F) -> F::Output {
        CURRENT.scope(self.clone(), work).await
    }

    /// Capture before spawning: Tokio task-local values do not cross task boundaries.
    pub fn inherit<F: Future>(work: F) -> impl Future<Output = F::Output> {
        let current = CURRENT.try_with(Clone::clone).ok();
        crate::RequestClock::inherit(async move {
            match current {
                Some(current) => current.scope(work).await,
                None => work.await,
            }
        })
    }

    pub fn recorded() {
        let _ = CURRENT.try_with(|current| current.state.lock().unwrap().recorded = true);
    }

    pub fn metadata(metadata: &crate::RequestMetadata) {
        let _ = CURRENT.try_with(|current| {
            let mut state = current.state.lock().unwrap();
            state.record.model = metadata
                .model
                .as_deref()
                .filter(|model| codex2api_common::valid_model(model))
                .map(str::to_owned);
            state.record.stream = metadata.stream;
            state.record.reasoning_effort = metadata.reasoning_effort.clone();
            state.record.service_tier = metadata
                .service_tier
                .as_ref()
                .map(|value| value.chars().take(64).collect());
            state.record.image_size = metadata.image_size.clone();
        });
    }

    pub fn diagnostic(message: &str) {
        let _ = CURRENT.try_with(|current| {
            current.state.lock().unwrap().diagnostic = Some(diagnostic_text(message, 512));
        });
    }

    pub fn error(error: &(dyn std::error::Error + 'static)) {
        Self::diagnostic(&error_diagnostic(error));
    }

    pub fn needs_record(&self, status: http::StatusCode) -> bool {
        (status.is_client_error() || status.is_server_error())
            && !self.state.lock().unwrap().recorded
    }

    /// Inspect a bounded error prefix and replay every consumed byte/error unchanged.
    pub async fn capture<S, E>(
        &self,
        storage: &Storage,
        status: http::StatusCode,
        source: S,
    ) -> Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send>>
    where
        S: Stream<Item = Result<Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        let mut source = Box::pin(source);
        let mut consumed = Vec::new();
        let mut prefix = Vec::new();
        while prefix.len() < 8192 {
            let Ok(Some(chunk)) =
                tokio::time::timeout(std::time::Duration::from_secs(1), source.next()).await
            else {
                break;
            };
            let failed = chunk.is_err();
            if let Ok(bytes) = &chunk {
                prefix.extend_from_slice(&bytes[..bytes.len().min(8192 - prefix.len())]);
            }
            consumed.push(chunk);
            if failed {
                break;
            }
        }
        let mut record = {
            let state = self.state.lock().unwrap();
            let mut record = state.record.clone();
            let value = serde_json::from_slice(&prefix).unwrap_or_default();
            let failure = crate::ResponseFailure::from_error(Some(status.as_u16()), &value);
            record.error_code = Some(failure.code.unwrap_or_else(|| match status.as_u16() {
                413 => "request_too_large".into(),
                400 => "invalid_request_error".into(),
                415 => "unsupported_encoding".into(),
                500..=599 => "internal_error".into(),
                _ => "request_rejected".into(),
            }));
            let message = state
                .diagnostic
                .clone()
                .or(failure.message)
                .unwrap_or_else(|| {
                    if prefix.is_empty() {
                        status.to_string()
                    } else {
                        String::from_utf8_lossy(&prefix).into_owned()
                    }
                });
            record.error_message = Some(diagnostic_text(&message, 512));
            record
        };
        record.status = "failed".into();
        record.http_status = Some(i64::from(status.as_u16()));
        record.total_ms = Some(self.started.elapsed().as_millis().min(i64::MAX as u128) as i64);
        if let Err(error) = storage.insert_rejected_usage(&record).await {
            tracing::error!(%error, request_id=%record.id, endpoint=%record.endpoint, "Could not record rejected request");
        }
        Box::pin(futures::stream::iter(consumed).chain(source))
    }
}

pub fn diagnostic_text(value: &str, limit: usize) -> String {
    let mut hide_next = false;
    value
        .split_whitespace()
        .map(|word| {
            let lower = word.to_ascii_lowercase();
            let secret = hide_next
                || lower.starts_with("sk-")
                || lower.starts_with("c2a_")
                || lower.starts_with("c2rt_")
                || lower.starts_with("eyj")
                || lower.contains("://");
            hide_next = lower.trim_end_matches(':') == "bearer";
            if secret { "[REDACTED]" } else { word }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

pub fn error_diagnostic(error: &(dyn std::error::Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    for _ in 0..8 {
        let Some(cause) = source else { break };
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    diagnostic_text(&message, 512)
}

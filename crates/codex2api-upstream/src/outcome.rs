//! A generation's outcome comes from Responses events, independently of the
//! HTTP/SSE handshake or the lifetime of a reusable WebSocket connection.
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Authentication,
    Permission,
    RateLimit,
    QuotaExhausted,
    LimitUnknown,
    InvalidRequest,
    Timeout,
    Transport,
    Upstream,
    Incomplete,
}
impl FailureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Authentication => "authentication",
            Self::Permission => "permission",
            Self::RateLimit => "rate_limit",
            Self::QuotaExhausted => "quota_exhausted",
            Self::LimitUnknown => "limit_unknown",
            Self::InvalidRequest => "invalid_request",
            Self::Timeout => "timeout",
            Self::Transport => "transport",
            Self::Upstream => "upstream",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseFailure {
    pub kind: FailureKind,
    /// Error status reported by the peer or defined by a known error code.
    /// Never copy HTTP 200/101 into this field. Unknown failures stay unknown.
    pub status: Option<u16>,
    pub code: Option<String>,
    pub message: Option<String>,
}

impl ResponseFailure {
    pub fn new(status: Option<u16>, code: Option<&str>, message: Option<&str>) -> Self {
        use FailureKind as K;
        let known = match code {
            Some("rate_limit_exceeded" | "slow_down" | "rate_limit_error") => {
                Some((K::RateLimit, 429))
            }
            Some(
                "insufficient_quota"
                | "usage_limit_reached"
                | "credit_balance_exhausted"
                | "organization_spend_limit_exceeded"
                | "project_spend_limit_exceeded"
                | "virtual_quota_exceeded",
            ) => Some((K::QuotaExhausted, 429)),
            Some("context_length_exceeded" | "invalid_prompt" | "invalid_request_error") => {
                Some((K::InvalidRequest, 400))
            }
            Some("usage_not_included" | "permission_error") => Some((K::Permission, 403)),
            Some("upstream_timeout" | "stream_idle_timeout") => Some((K::Timeout, 504)),
            Some(
                "upstream_connection_failed"
                | "upstream_transport_error"
                | "upstream_stream_error"
                | "stream_error"
                | "upstream_websocket_read_error"
                | "upstream_websocket_write_error"
                | "upstream_websocket_closed"
                | "upstream_incomplete",
            ) => Some((K::Transport, 502)),
            Some("server_error" | "internal_error") => Some((K::Upstream, 500)),
            _ => None,
        };
        let reported = status.filter(|s| (400..=599).contains(s));
        let kind = match reported {
            Some(401) => K::Authentication,
            Some(403) => K::Permission,
            Some(408 | 504) => K::Timeout,
            Some(429) => match known {
                Some((K::RateLimit, _)) => K::RateLimit,
                Some((K::QuotaExhausted, _)) => K::QuotaExhausted,
                _ => K::LimitUnknown,
            },
            Some(400..=499) => K::InvalidRequest,
            Some(_) => match known {
                Some((K::Transport, _)) => K::Transport,
                _ => K::Upstream,
            },
            None => known.map_or(K::Upstream, |(kind, _)| kind),
        };
        Self {
            kind,
            status: reported.or_else(|| known.map(|(_, status)| status)),
            code: code.filter(|s| !s.is_empty()).map(str::to_owned),
            message: message.filter(|s| !s.is_empty()).map(str::to_owned),
        }
    }

    pub fn from_error(http_status: Option<u16>, value: &Value) -> Self {
        let error = value
            .pointer("/response/error")
            .filter(|v| !v.is_null())
            .or_else(|| value.get("error").filter(|v| !v.is_null()))
            .or_else(|| {
                value
                    .get("detail")
                    .filter(|_| http_status.is_some_and(|s| s >= 400))
            })
            .unwrap_or(value);
        let status = [
            value.get("status"),
            value.get("status_code"),
            error.get("status"),
            error.get("status_code"),
        ]
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .filter_map(|s| u16::try_from(s).ok())
        .find(|s| (400..=599).contains(s))
        .or(http_status);
        let code = error.get("code").and_then(Value::as_str).or_else(|| {
            error
                .get("type")
                .and_then(Value::as_str)
                .filter(|s| *s != "error")
        });
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| error.as_str());
        Self::new(status, code, message)
    }

    pub fn authentication_invalid(&self) -> bool {
        self.status == Some(401)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseOutcome {
    Completed,
    Failed(ResponseFailure),
    Incomplete(ResponseFailure),
    Cancelled,
}
impl ResponseOutcome {
    /// The same event interpretation is used by JSON, SSE and WebSocket ledgers.
    /// There is no terminal outcome for response.created/in_progress, deltas,
    /// output_item.done, rate-limit notifications, heartbeats or an HTTP 200.
    pub fn from_value(value: &Value, http_status: Option<u16>) -> Option<Self> {
        let response = value.get("response").unwrap_or(value);
        let event = value.get("type").and_then(Value::as_str);
        let status = response.get("status").and_then(Value::as_str);
        let error = response
            .get("error")
            .or_else(|| value.get("error"))
            .is_some_and(|v| !v.is_null());
        if error
            || matches!(event, Some("error" | "response.failed"))
            || status == Some("failed")
            || http_status.is_some_and(|s| s >= 400)
        {
            return Some(Self::Failed(ResponseFailure::from_error(
                http_status,
                value,
            )));
        }
        if event == Some("response.incomplete") || status == Some("incomplete") {
            let reason = response
                .pointer("/incomplete_details/reason")
                .and_then(Value::as_str);
            return Some(Self::Incomplete(ResponseFailure {
                kind: FailureKind::Incomplete,
                status: None,
                code: reason.map(str::to_owned),
                message: Some("上游生成未完成".into()),
            }));
        }
        if event == Some("response.cancelled") || status == Some("cancelled") {
            return Some(Self::Cancelled);
        }
        if matches!(event, Some("response.completed" | "response.done"))
            || status == Some("completed")
        {
            return Some(Self::Completed);
        }
        None
    }

    pub fn record_status(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed(_) => "failed",
            Self::Incomplete(_) => "incomplete",
            Self::Cancelled => "client_stopped",
        }
    }
    pub fn failure(&self) -> Option<&ResponseFailure> {
        match self {
            Self::Failed(f) | Self::Incomplete(f) => Some(f),
            _ => None,
        }
    }
}

/// Once terminal, late frames/transport closure cannot overwrite generation data.
#[derive(Debug, Default)]
pub struct ResponseLifecycle {
    outcome: Option<ResponseOutcome>,
}
impl ResponseLifecycle {
    pub fn outcome(&self) -> Option<&ResponseOutcome> {
        self.outcome.as_ref()
    }
    pub fn observe(&mut self, value: &Value, http_status: Option<u16>) -> bool {
        if self.outcome.is_some() {
            return false;
        }
        self.outcome = ResponseOutcome::from_value(value, http_status);
        self.outcome.is_some()
    }
    pub fn fail(&mut self, failure: ResponseFailure) {
        if self.outcome.is_none() {
            self.outcome = Some(ResponseOutcome::Failed(failure));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn events_define_outcomes_and_terminal_results_are_immutable() {
        let mut state = ResponseLifecycle::default();
        for event in [
            "response.created",
            "response.in_progress",
            "response.output_text.delta",
            "response.output_item.done",
            "rate_limits.updated",
        ] {
            assert!(!state.observe(&json!({"type":event}), Some(200)));
        }
        assert!(state.observe(&json!({"type":"response.failed","response":{"error":{"code":"insufficient_quota","message":"exhausted"}}}), Some(200)));
        assert_eq!(
            state.outcome().unwrap().failure().unwrap().kind,
            FailureKind::QuotaExhausted
        );
        assert_eq!(
            state.outcome().unwrap().failure().unwrap().status,
            Some(429)
        );
        assert!(!state.observe(&json!({"type":"response.completed"}), Some(200)));
        state.fail(ResponseFailure::new(None, Some("stream_error"), None));
        assert_eq!(
            state.outcome().unwrap().failure().unwrap().kind,
            FailureKind::QuotaExhausted
        );
    }
    #[test]
    fn classification_requires_structured_evidence() {
        for (status, code, kind) in [
            (Some(401), "unknown", FailureKind::Authentication),
            (Some(403), "invalid_token", FailureKind::Permission),
            (Some(429), "rate_limit_exceeded", FailureKind::RateLimit),
            (
                Some(429),
                "usage_limit_reached",
                FailureKind::QuotaExhausted,
            ),
            (Some(429), "unknown", FailureKind::LimitUnknown),
            (None, "slow_down", FailureKind::RateLimit),
            (
                None,
                "credit_balance_exhausted",
                FailureKind::QuotaExhausted,
            ),
            (None, "upstream_stream_error", FailureKind::Transport),
            (None, "invalid_token", FailureKind::Upstream),
        ] {
            let failure = ResponseFailure::new(status, Some(code), Some("401 quota rate limit"));
            assert_eq!(failure.kind, kind);
            assert_eq!(failure.authentication_invalid(), status == Some(401));
        }
        let failure = ResponseFailure::from_error(
            Some(200),
            &json!({"type":"error","status":429,"error":{"code":"insufficient_quota"}}),
        );
        assert_eq!(failure.status, Some(429));
        assert_eq!(failure.kind, FailureKind::QuotaExhausted);
        assert!(
            ResponseFailure::new(None, Some("new_error"), None)
                .status
                .is_none()
        );
    }
}

use axum::{
    Json,
    response::{IntoResponse, Response},
};
use codex2api_upstream::{convert, executor::ExecutorError, inbound::Inbound};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
pub struct ApiError {
    pub(crate) status: StatusCode,
    /// What the caller receives.
    pub(crate) body: Value,
    pub(crate) headers: HeaderMap,
    pub(crate) diagnostic: Option<String>,
    /// The real supplier failure behind `body`, kept for the administrator ledger.
    pub(crate) cause: Option<codex2api_upstream::ResponseFailure>,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "API request failed: {}", self.status)
    }
}
impl std::error::Error for ApiError {}

impl ApiError {
    pub(crate) fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        let kind = match status.as_u16() {
            401 => "authentication_error",
            403 => "permission_error",
            429 => "rate_limit_error",
            400..=499 => "invalid_request_error",
            _ => "server_error",
        };
        Self {
            status,
            body: json!({"error":{"type":kind,"code":code,"message":message.into(),"param":null}}),
            headers: HeaderMap::new(),
            diagnostic: None,
            cause: None,
        }
    }
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_request_error", message)
    }
    pub(crate) fn invalid_key() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "invalid_api_key",
            "A valid API key is required in Authorization: Bearer.",
        )
    }
    pub(crate) fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "The service could not complete this request.",
        )
    }
    pub(crate) fn with_diagnostic(mut self, message: &str) -> Self {
        let message = codex2api_upstream::request_failure::diagnostic_text(message, 512);
        codex2api_upstream::RequestFailureLog::diagnostic(&message);
        self.diagnostic = Some(message);
        self
    }
    pub(crate) fn upstream_incomplete() -> Self {
        Self::new(
            StatusCode::BAD_GATEWAY,
            "upstream_incomplete",
            "The upstream stream ended without a complete response.",
        )
    }
    /// No supplier in the pool can serve the request now.
    pub(crate) fn no_supplier(model_not_found: bool) -> Self {
        if model_not_found {
            return Self::new(
                StatusCode::NOT_FOUND,
                "model_not_found",
                "No supplier in the pool is configured for this model.",
            );
        }
        let mut error = Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "no_available_supplier",
            "No supplier is available for this request. Please retry later.",
        );
        error
            .headers
            .insert("retry-after", http::HeaderValue::from_static("5"));
        error
    }
    /// A supplier's reply or event that the caller receives in its own format, with
    /// the supplier's own status; `cause` keeps the real failure for the ledger.
    pub(crate) fn wire_failure(inbound: Inbound, status: Option<u16>, value: &Value) -> Self {
        let cause = codex2api_upstream::ResponseFailure::from_error(status, value);
        let status = status.or(cause.status).unwrap_or(502);
        let (status, body) = convert::escaped_reply(inbound, status, value);
        Self {
            status,
            body,
            headers: HeaderMap::new(),
            diagnostic: None,
            cause: Some(cause),
        }
    }
    /// A supplier failure event or reply body as the caller sees it.
    pub(crate) fn upstream_value(value: &Value) -> Self {
        Self::wire_failure(Inbound::Responses, None, value)
    }
    pub(crate) fn executor(error: ExecutorError, inbound: Inbound) -> Self {
        match error {
            ExecutorError::NoSupplier { model_not_found } => Self::no_supplier(model_not_found),
            ExecutorError::Busy => {
                let mut error = Self::no_supplier(false);
                error
                    .headers
                    .insert("retry-after", http::HeaderValue::from_static("1"));
                error
            }
            ExecutorError::ContextRequired => Self::new(
                StatusCode::BAD_REQUEST,
                "previous_response_not_found",
                "The previous response is no longer available. Send the full conversation without previous_response_id.",
            ),
            ExecutorError::ConsumerInvalid => Self::invalid_key(),
            ExecutorError::Client { status, body, .. } => {
                let mut error = Self::wire_failure(inbound, Some(status), &body);
                (error.status, error.body) = convert::client_error(inbound, status, &body);
                error
            }
            ExecutorError::Stopped { status, body, .. } => {
                Self::wire_failure(inbound, Some(status), &body)
            }
            ExecutorError::Convert(error) => error.into(),
            ExecutorError::Upstream(error) => error.into(),
            ExecutorError::Storage(error) => error.into(),
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.body)).into_response();
        response.headers_mut().extend(self.headers);
        response.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        response
            .headers_mut()
            .insert("cache-control", "no-store".parse().unwrap());
        response
    }
}
impl From<crate::runtime::LimitError> for ApiError {
    fn from(value: crate::runtime::LimitError) -> Self {
        let (code, retry_after) = match value {
            crate::runtime::LimitError::Storage(error) => return error.into(),
            crate::runtime::LimitError::Limited { code, retry_after } => (code, retry_after),
        };
        let mut error = Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            code,
            "The user has reached an API group or model rate limit.",
        );
        error
            .headers
            .insert("retry-after", retry_after.to_string().parse().unwrap());
        error
    }
}
impl From<codex2api_upstream::scheduler::SchedulerError> for ApiError {
    fn from(value: codex2api_upstream::scheduler::SchedulerError) -> Self {
        Self::executor(ExecutorError::from(value), Inbound::Responses)
    }
}
impl From<codex2api_storage::StorageError> for ApiError {
    fn from(value: codex2api_storage::StorageError) -> Self {
        let diagnostic = codex2api_upstream::request_failure::error_diagnostic(&value);
        use codex2api_storage::StorageError as E;
        let error = match value {
            E::Api { code, message } => {
                let status = match code.as_ref() {
                    "invalid_api_key" => StatusCode::UNAUTHORIZED,
                    "insufficient_balance" | "api_wallet_insufficient" => {
                        StatusCode::PAYMENT_REQUIRED
                    }
                    "api_model_unavailable" | "model_not_available" | "api_group_disabled" => {
                        StatusCode::FORBIDDEN
                    }
                    "pricing_unavailable"
                    | "model_price_unavailable"
                    | "operation_price_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
                    "api_group_changed" => StatusCode::CONFLICT,
                    _ => StatusCode::BAD_REQUEST,
                };
                Self::new(status, code.as_ref(), message)
            }
            E::Runtime => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "runtime_state_unavailable",
                "Request scheduling is temporarily unavailable.",
            ),
            other => {
                tracing::error!(error=%other,"API gateway storage failure");
                Self::internal()
            }
        };
        error.with_diagnostic(&diagnostic)
    }
}
/// Channel errors that escape the executor: the caller's own request faults, this
/// service's faults, and transport faults after a reply was accepted.
impl From<codex2api_upstream::UpstreamError> for ApiError {
    fn from(value: codex2api_upstream::UpstreamError) -> Self {
        use codex2api_upstream::UpstreamError as E;
        match value {
            // The caller's own request: the message is meant for the caller.
            E::InvalidRequest(message) => Self::bad_request(message),
            E::RequestTooLarge => Self::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request_too_large",
                "Request body exceeds the size limit.",
            ),
            E::UnsupportedEncoding => Self::new(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_encoding",
                "Unsupported request Content-Encoding.",
            ),
            E::Storage(error) => error.into(),
            E::Internal(message) => {
                tracing::error!(%message, "API gateway internal failure");
                Self::internal().with_diagnostic(&format!("internal error: {message}"))
            }
            E::Status { status, body, .. } => {
                let value: Value = serde_json::from_str(&body).unwrap_or_default();
                let cause = codex2api_upstream::ResponseFailure::from_error(Some(status), &value);
                let (status, body) = convert::escaped_reply(Inbound::Responses, status, &value);
                Self {
                    status,
                    body,
                    headers: HeaderMap::new(),
                    diagnostic: None,
                    cause: Some(cause),
                }
            }
            other => {
                let diagnostic = codex2api_upstream::request_failure::error_diagnostic(&other);
                let failure = other.failure();
                let (status, code, message) = match failure.kind {
                    codex2api_upstream::FailureKind::Timeout => (
                        StatusCode::GATEWAY_TIMEOUT,
                        "upstream_timeout",
                        "The upstream service timed out. Please try again later.",
                    ),
                    codex2api_upstream::FailureKind::Authentication
                    | codex2api_upstream::FailureKind::PaymentRequired
                    | codex2api_upstream::FailureKind::Permission
                    | codex2api_upstream::FailureKind::QuotaExhausted
                    | codex2api_upstream::FailureKind::RateLimit
                    | codex2api_upstream::FailureKind::LimitUnknown => (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "no_available_supplier",
                        "No supplier is available for this request. Please retry later.",
                    ),
                    _ => (
                        StatusCode::BAD_GATEWAY,
                        "upstream_error",
                        "The upstream service could not complete this request.",
                    ),
                };
                let mut error = Self::new(status, code, message).with_diagnostic(&diagnostic);
                error.cause = Some(failure);
                error
            }
        }
    }
}

impl From<convert::Error> for ApiError {
    fn from(error: convert::Error) -> Self {
        Self {
            status: error.status,
            body: error.body,
            headers: HeaderMap::new(),
            diagnostic: None,
            cause: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_failures_keep_their_status_and_identity_but_not_supplier_details() {
        let value = json!({"type":"error","status":400,"error":{"type":"invalid_request_error","code":"invalid_prompt","message":"bad prompt from org-123 secret@example.test"}});
        let error = ApiError::wire_failure(Inbound::Responses, Some(400), &value);
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.body["error"]["code"], "invalid_prompt");
        assert!(!error.body.to_string().contains("org-123"));
        assert!(!error.body.to_string().contains("secret@"));
        let messages = ApiError::wire_failure(Inbound::Messages, Some(400), &value);
        assert_eq!(messages.body["type"], "error");
        assert_eq!(messages.body["error"]["type"], "invalid_request_error");
        let exhausted = ApiError::executor(
            ExecutorError::NoSupplier {
                model_not_found: false,
            },
            Inbound::Chat,
        );
        assert_eq!(exhausted.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(exhausted.body["error"]["code"], "no_available_supplier");
    }
}

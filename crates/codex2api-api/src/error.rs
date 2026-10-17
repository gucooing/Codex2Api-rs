use axum::{
    Json,
    response::{IntoResponse, Response},
};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

pub type Result<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
pub struct ApiError {
    pub(crate) status: StatusCode,
    pub(crate) body: Value,
    pub(crate) headers: HeaderMap,
    pub(crate) diagnostic: Option<String>,
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
    pub(crate) fn upstream_value(value: &Value) -> Self {
        let failure = codex2api_upstream::ResponseFailure::from_error(None, value);
        let raw = value
            .pointer("/response/error")
            .filter(|v| !v.is_null())
            .or_else(|| value.get("error").filter(|v| !v.is_null()))
            .unwrap_or(value);
        let mut headers = HeaderMap::new();
        for source in [
            value.get("headers"),
            value.pointer("/response/headers"),
            raw.get("headers"),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(values) = source.as_object() {
                for (name, value) in values {
                    let Ok(name) = http::HeaderName::from_bytes(name.as_bytes()) else {
                        continue;
                    };
                    if !matches!(
                        name.as_str(),
                        "retry-after" | "retry-after-ms" | "x-should-retry"
                    ) {
                        continue;
                    }
                    let text = match value {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        _ => continue,
                    };
                    if let Ok(value) = http::HeaderValue::from_str(&text) {
                        headers.insert(name, value);
                    }
                }
            }
        }
        Self {
            status: StatusCode::from_u16(failure.status.unwrap_or(502))
                .unwrap_or(StatusCode::BAD_GATEWAY),
            body: json!({"error":codex2api_upstream::public_output::error(raw)}),
            headers,
            diagnostic: None,
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
        use codex2api_upstream::scheduler::SchedulerError as E;
        match value {
            E::Storage(error) => error.into(),
            E::Upstream(error) => error.into(),
            E::ConsumerInvalid => Self::invalid_key(),
            E::Rejected {
                status,
                code,
                message,
                ..
            } => {
                let mut error = Self::new(status, code.unwrap_or("supplier_unavailable"), message);
                if status == StatusCode::TOO_MANY_REQUESTS {
                    error.headers.insert("retry-after", "1".parse().unwrap());
                }
                error
            }
        }
    }
}
impl From<codex2api_storage::StorageError> for ApiError {
    fn from(value: codex2api_storage::StorageError) -> Self {
        let diagnostic = codex2api_upstream::request_failure::error_diagnostic(&value);
        use codex2api_storage::StorageError as E;
        let error = match value {
            E::Api { code, message } => {
                let status = match code.as_ref() {
                    "invalid_api_key" | "api_key_revoked" => StatusCode::UNAUTHORIZED,
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
impl From<codex2api_upstream::UpstreamError> for ApiError {
    fn from(value: codex2api_upstream::UpstreamError) -> Self {
        use codex2api_upstream::UpstreamError as E;
        match value {
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
            E::Status {
                status,
                body,
                headers,
                ..
            } => {
                let value: Value = serde_json::from_str(&body).unwrap_or_default();
                let mut error = Self::upstream_value(&value);
                if let Ok(status) = StatusCode::from_u16(status)
                    && (status.is_client_error() || status.is_server_error())
                {
                    error.status = status;
                }
                for name in ["retry-after", "retry-after-ms", "x-should-retry"] {
                    if let Some(value) = headers.get(name) {
                        error.headers.insert(name, value.clone());
                    }
                }
                error
            }
            other => {
                let diagnostic = codex2api_upstream::request_failure::error_diagnostic(&other);
                let failure = other.failure();
                let mut response = Self::new(
                    StatusCode::from_u16(failure.status.unwrap_or(502))
                        .unwrap_or(StatusCode::BAD_GATEWAY),
                    failure.code.as_deref().unwrap_or("upstream_error"),
                    "The upstream service could not complete this request.",
                )
                .with_diagnostic(&diagnostic);
                if let Some(headers) = other.retry_headers() {
                    for name in ["retry-after", "retry-after-ms", "x-should-retry"] {
                        if let Some(value) = headers.get(name) {
                            response.headers.insert(name, value.clone());
                        }
                    }
                }
                response
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn api_rewrite_errors_keep_status_and_retry_headers() {
        let value = json!({"type":"error","status":429,"headers":{"Retry-After":"7","Authorization":"private"},"error":{"code":"rate_limit_exceeded","message":"private supplier"}});
        let error = ApiError::from(codex2api_upstream::UpstreamError::Status {
            status: 200,
            body: value.to_string(),
            headers: HeaderMap::new(),
            auth_revision: None,
        });
        assert_eq!(error.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(error.headers["retry-after"], "7");
        assert!(!error.headers.contains_key("authorization"));
        assert_eq!(error.body["error"]["code"], "rate_limit_exceeded");
        assert!(!error.body.to_string().contains("private"));
    }
}

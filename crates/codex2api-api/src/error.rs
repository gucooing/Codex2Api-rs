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
    /// A supplier failure as the caller sees it after the gateway's handling. Retry
    /// advice is forwarded only when the public view is throttling.
    fn supplier(
        cause: codex2api_upstream::ResponseFailure,
        public: codex2api_upstream::public_output::PublicFailure,
        error: Value,
        retry: Option<&HeaderMap>,
    ) -> Self {
        let mut headers = HeaderMap::new();
        if public.throttled()
            && let Some(retry) = retry
        {
            for name in ["retry-after", "retry-after-ms", "x-should-retry"] {
                if let Some(value) = retry.get(name) {
                    headers.insert(name, value.clone());
                }
            }
        }
        Self {
            status: public.status_code(),
            body: json!({ "error": error }),
            headers,
            diagnostic: None,
            cause: Some(cause),
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
    /// A supplier failure event or reply body as the caller sees it.
    pub(crate) fn upstream_value(value: &Value) -> Self {
        Self::upstream_reply(None, value, &HeaderMap::new())
    }
    /// `http_status` and `headers` belong to the supplier's HTTP reply, when there was one.
    fn upstream_reply(http_status: Option<u16>, value: &Value, headers: &HeaderMap) -> Self {
        let (public, error) = codex2api_upstream::public_output::failure(http_status, value);
        let raw = value
            .pointer("/response/error")
            .filter(|v| !v.is_null())
            .or_else(|| value.get("error").filter(|v| !v.is_null()))
            .unwrap_or(value);
        let mut retry = headers.clone();
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
                    ) || retry.contains_key(&name)
                    {
                        continue;
                    }
                    let text = match value {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        _ => continue,
                    };
                    if let Ok(value) = http::HeaderValue::from_str(&text) {
                        retry.insert(name, value);
                    }
                }
            }
        }
        Self::supplier(
            codex2api_upstream::ResponseFailure::from_error(http_status, value),
            public,
            error,
            Some(&retry),
        )
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
            // Pool state is the service's own; the caller sees its public view.
            E::Rejected {
                status,
                code,
                message,
                ..
            } => {
                let cause = codex2api_upstream::ResponseFailure::new(
                    Some(status.as_u16()),
                    code,
                    Some(&message),
                );
                let public = codex2api_upstream::public_output::PublicFailure::new(&cause);
                let retry = public.throttled().then(|| {
                    HeaderMap::from_iter([(http::header::RETRY_AFTER, "1".parse().unwrap())])
                });
                let error = public.error();
                Self::supplier(cause, public, error, retry.as_ref())
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
            E::Status {
                status,
                body,
                headers,
                ..
            } => Self::upstream_reply(
                Some(status),
                &serde_json::from_str(&body).unwrap_or_default(),
                &headers,
            ),
            other => {
                let diagnostic = codex2api_upstream::request_failure::error_diagnostic(&other);
                let public = other.public_failure();
                let error = public.error();
                Self::supplier(other.failure(), public, error, other.retry_headers())
                    .with_diagnostic(&diagnostic)
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

impl From<codex2api_upstream::protocol::Error> for ApiError {
    fn from(error: codex2api_upstream::protocol::Error) -> Self {
        Self {
            status: error.status,
            body: error.body,
            headers: HeaderMap::new(),
            diagnostic: None,
            cause: None,
        }
    }
}

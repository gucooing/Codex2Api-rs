use axum::{
    Json,
    response::{IntoResponse, Response},
};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

pub type Result<T> = std::result::Result<T, GatewayError>;

#[derive(Debug)]
pub struct GatewayError {
    pub(crate) status: StatusCode,
    pub(crate) body: Value,
    pub(crate) headers: HeaderMap,
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "API request failed: {}", self.status)
    }
}
impl std::error::Error for GatewayError {}

impl GatewayError {
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
            .or_else(|| value.get("error"))
            .unwrap_or(value);
        Self {
            status: StatusCode::from_u16(failure.status.unwrap_or(502))
                .unwrap_or(StatusCode::BAD_GATEWAY),
            body: json!({"error":codex2api_upstream::public_output::error(raw)}),
            headers: HeaderMap::new(),
        }
    }
}
impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.body)).into_response();
        response.headers_mut().extend(self.headers);
        response
            .headers_mut()
            .insert("cache-control", "no-store".parse().unwrap());
        response
    }
}
impl From<crate::runtime::LimitError> for GatewayError {
    fn from(value: crate::runtime::LimitError) -> Self {
        let mut error = Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            value.code,
            "The user has reached an API group or model rate limit.",
        );
        error.headers.insert(
            "retry-after",
            value.retry_after.to_string().parse().unwrap(),
        );
        error
    }
}
impl From<codex2api_scheduler::SchedulerError> for GatewayError {
    fn from(value: codex2api_scheduler::SchedulerError) -> Self {
        use codex2api_scheduler::SchedulerError as E;
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
impl From<codex2api_storage::StorageError> for GatewayError {
    fn from(value: codex2api_storage::StorageError) -> Self {
        use codex2api_storage::StorageError as E;
        match value {
            E::Api { code, message } => {
                let status = match code.as_ref() {
                    "invalid_api_key" | "api_key_revoked" => StatusCode::UNAUTHORIZED,
                    "insufficient_balance" | "api_wallet_insufficient" => {
                        StatusCode::PAYMENT_REQUIRED
                    }
                    "api_model_unavailable" | "model_not_available" | "api_group_disabled" => {
                        StatusCode::FORBIDDEN
                    }
                    "pricing_unavailable" | "model_price_unavailable" => {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
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
        }
    }
}
impl From<codex2api_upstream::UpstreamError> for GatewayError {
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
                error.status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
                for name in ["retry-after", "retry-after-ms"] {
                    if let Some(value) = headers.get(name) {
                        error.headers.insert(name, value.clone());
                    }
                }
                error
            }
            other => {
                let failure = other.failure();
                Self::new(
                    StatusCode::from_u16(failure.status.unwrap_or(502))
                        .unwrap_or(StatusCode::BAD_GATEWAY),
                    failure.code.as_deref().unwrap_or("upstream_error"),
                    "The upstream service could not complete this request.",
                )
            }
        }
    }
}

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::{Value, json};

use codex2api_accounts::AccountError;
use codex2api_storage::StorageError;
use codex2api_upstream::UpstreamError;

pub type Result<T> = std::result::Result<T, ApiError>;

/// Public API failure mapped to OpenAI-style `{ "error": { ... } }` JSON.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Service(#[from] codex2api_service::ServiceError),
    #[error("{message}")]
    OpenAi {
        status: StatusCode,
        error_type: &'static str,
        message: String,
        code: Option<&'static str>,
    },
    #[error(transparent)]
    Upstream(#[from] UpstreamError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    SupplierAccount(#[from] AccountError),
}

impl ApiError {
    pub fn openai(
        status: StatusCode,
        error_type: &'static str,
        message: impl Into<String>,
        code: Option<&'static str>,
    ) -> Self {
        Self::OpenAi {
            status,
            error_type,
            message: message.into(),
            code,
        }
    }

    pub fn missing_token() -> Self {
        Self::openai(
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "Provide a virtual account access token using Authorization: Bearer.",
            Some("missing_token"),
        )
    }

    pub fn invalid_token() -> Self {
        Self::openai(
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "Invalid or expired virtual account access token.",
            Some("invalid_token"),
        )
    }

    pub fn account_disabled() -> Self {
        Self::openai(
            StatusCode::FORBIDDEN,
            "permission_error",
            "This account is disabled.",
            Some("account_disabled"),
        )
    }

    pub fn account_not_ready() -> Self {
        Self::openai(
            StatusCode::FORBIDDEN,
            "permission_error",
            "This account is not ready. Complete ChatGPT login before using the API.",
            Some("account_not_ready"),
        )
    }

    pub fn account_not_authenticated() -> Self {
        Self::openai(
            StatusCode::FORBIDDEN,
            "permission_error",
            "This account is not authenticated with ChatGPT.",
            Some("account_not_authenticated"),
        )
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::openai(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            message,
            None,
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::openai(
            StatusCode::INTERNAL_SERVER_ERROR,
            "api_error",
            message,
            None,
        )
    }
}

#[derive(Debug, Serialize)]
pub struct OpenAiErrorBody {
    pub error: OpenAiError,
}

#[derive(Debug, Serialize)]
pub struct OpenAiError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
}

impl OpenAiErrorBody {
    pub fn new(error_type: &str, message: impl Into<String>, code: Option<&str>) -> Self {
        Self {
            error: OpenAiError {
                message: message.into(),
                error_type: error_type.to_string(),
                code: code.map(str::to_string),
                param: None,
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Service(error) => service_error_response(error),
            Self::OpenAi {
                status,
                error_type,
                message,
                code,
            } => openai_response(status, error_type, message, code),
            Self::Upstream(err) => upstream_error_response(err),
            Self::Storage(StorageError::AccountNotFound(_)) => {
                ApiError::invalid_token().into_response()
            }
            Self::Storage(err) => {
                tracing::error!(error = %err, "storage error");
                openai_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "api_error",
                    "Internal server error.",
                    None,
                )
            }
            Self::SupplierAccount(AccountError::NotFound(_)) => {
                ApiError::invalid_token().into_response()
            }
            Self::SupplierAccount(err) => {
                tracing::error!(error = %err, "account error");
                openai_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "api_error",
                    "Internal server error.",
                    None,
                )
            }
        }
    }
}

pub fn openai_response(
    status: StatusCode,
    error_type: &str,
    message: impl Into<String>,
    code: Option<&str>,
) -> Response {
    (
        status,
        Json(OpenAiErrorBody::new(error_type, message, code)),
    )
        .into_response()
}

pub fn openai_json(error_type: &str, message: impl Into<String>, code: Option<&str>) -> Value {
    serde_json::to_value(OpenAiErrorBody::new(error_type, message, code)).unwrap_or_else(
        |_| json!({ "error": { "message": "Internal server error.", "type": "api_error" } }),
    )
}

/// Map an upstream HTTP error body to OpenAI-style JSON, preserving upstream
/// `{ "error": ... }` payloads when present.
pub fn map_upstream_status_body(status: StatusCode, body: &str) -> Response {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if value.get("error").is_some() {
            return (status, Json(value)).into_response();
        }
        if let Some(message) = value
            .get("message")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            return openai_response(status, error_type_for_status(status), message, None);
        }
    }
    let trimmed = body.trim();
    let message = if trimmed.is_empty() {
        default_message_for_status(status).to_string()
    } else {
        truncate(trimmed, 2048)
    };
    openai_response(status, error_type_for_status(status), message, None)
}

pub fn upstream_error_response(err: UpstreamError) -> Response {
    match err {
        UpstreamError::InvalidRequest(message) => ApiError::bad_request(message).into_response(),
        UpstreamError::RequestTooLarge => openai_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "invalid_request_error",
            "Request body is too large.",
            None,
        ),
        UpstreamError::UnsupportedEncoding => openai_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "invalid_request_error",
            "Supported request encodings are identity and zstd.",
            None,
        ),
        UpstreamError::Unauthorized => openai_response(
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "Upstream ChatGPT authentication failed.",
            Some("upstream_unauthorized"),
        ),
        UpstreamError::MissingAccessToken(_) => {
            ApiError::account_not_authenticated().into_response()
        }
        UpstreamError::Refresh(err) => {
            tracing::warn!(error = %err, "upstream token refresh failed");
            openai_response(
                StatusCode::UNAUTHORIZED,
                "authentication_error",
                "Failed to refresh ChatGPT access token.",
                Some("upstream_refresh_failed"),
            )
        }
        UpstreamError::Status { status, body } => {
            let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
            map_upstream_status_body(status, &body)
        }
        UpstreamError::StreamIdleTimeout => openai_response(
            StatusCode::GATEWAY_TIMEOUT,
            "api_error",
            "Upstream SSE stream idle timeout.",
            Some("stream_idle_timeout"),
        ),
        UpstreamError::Stream(message) => openai_response(
            StatusCode::BAD_GATEWAY,
            "api_error",
            format!("Upstream SSE stream error: {message}"),
            Some("stream_error"),
        ),
        UpstreamError::Http(err) => {
            tracing::error!(error = %err, "upstream HTTP error");
            openai_response(
                StatusCode::BAD_GATEWAY,
                "api_error",
                "Failed to reach upstream Codex servers.",
                Some("upstream_http_error"),
            )
        }
        other => {
            tracing::error!(error = %other, "upstream error");
            openai_response(
                StatusCode::BAD_GATEWAY,
                "api_error",
                other.to_string(),
                None,
            )
        }
    }
}

pub fn error_type_for_status(status: StatusCode) -> &'static str {
    match status.as_u16() {
        400 | 404 | 409 | 413 | 415 | 422 => "invalid_request_error",
        401 => "authentication_error",
        403 => "permission_error",
        429 => "rate_limit_error",
        _ => "api_error",
    }
}

fn default_message_for_status(status: StatusCode) -> &'static str {
    match status.as_u16() {
        400 => "Bad request.",
        401 => "Unauthorized.",
        403 => "Forbidden.",
        404 => "Not found.",
        409 => "Conflict.",
        429 => "Rate limit exceeded.",
        500..=599 => "Upstream server error.",
        _ => "Request failed.",
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

fn service_error_response(error: codex2api_service::ServiceError) -> Response {
    use codex2api_core::PolicyError;
    use codex2api_service::ServiceError;
    let (status, kind, code) = match &error {
        ServiceError::Storage(_) => {
            return match error {
                ServiceError::Storage(e) => ApiError::Storage(e).into_response(),
                _ => unreachable!(),
            };
        }
        ServiceError::BudgetExceeded => (
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limit_error",
            "virtual_quota_exceeded",
        ),
        ServiceError::PricingUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "server_error",
            "model_pricing_unavailable",
        ),
        ServiceError::Policy(PolicyError::InvalidModel) => (
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "invalid_model",
        ),
        ServiceError::Policy(PolicyError::InvalidModelScope) => (
            StatusCode::CONFLICT,
            "configuration_error",
            "invalid_model_policy",
        ),
        ServiceError::Policy(PolicyError::UnsupportedProvider) => (
            StatusCode::NOT_IMPLEMENTED,
            "server_error",
            "provider_unavailable",
        ),
        ServiceError::Policy(PolicyError::ProviderMismatch) => (
            StatusCode::FORBIDDEN,
            "permission_error",
            "provider_mismatch",
        ),
        ServiceError::Policy(PolicyError::SubscriptionRequired) => (
            StatusCode::FORBIDDEN,
            "permission_error",
            "subscription_required",
        ),
        ServiceError::Policy(PolicyError::ModelNotEntitled) => (
            StatusCode::FORBIDDEN,
            "permission_error",
            "model_not_entitled",
        ),
        ServiceError::Policy(PolicyError::ModelUnavailable) => (
            StatusCode::FORBIDDEN,
            "permission_error",
            "model_unavailable",
        ),
    };
    openai_response(status, kind, error.to_string(), Some(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_json_shape() {
        let value = openai_json(
            "authentication_error",
            "Invalid or expired virtual account access token.",
            Some("invalid_token"),
        );
        assert_eq!(value["error"]["type"], "authentication_error");
        assert_eq!(value["error"]["code"], "invalid_token");
        assert_eq!(
            value["error"]["message"],
            "Invalid or expired virtual account access token."
        );
    }

    #[test]
    fn preserves_upstream_error_object() {
        let body = r#"{"error":{"message":"overloaded","type":"server_error"}}"#;
        let response = map_upstream_status_body(StatusCode::SERVICE_UNAVAILABLE, body);
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::{Value, json};

use codex2api_storage::StorageError;
use codex2api_suppliers::AccountError;
use codex2api_upstream::executor::ExecutorError;
use codex2api_upstream::inbound::Inbound;
use codex2api_upstream::{UpstreamError, convert};

pub type Result<T> = std::result::Result<T, OAuthError>;

impl From<codex2api_upstream::scheduler::SchedulerError> for OAuthError {
    fn from(error: codex2api_upstream::scheduler::SchedulerError) -> Self {
        Self::executor(ExecutorError::from(error))
    }
}

impl OAuthError {
    /// No supplier in the pool can serve the request now.
    pub(crate) fn no_supplier(model_not_found: bool) -> Self {
        if model_not_found {
            Self::openai(
                StatusCode::NOT_FOUND,
                "invalid_request_error",
                "No supplier in the pool is configured for this model.",
                Some("model_not_found"),
            )
        } else {
            crate::pool_execution::exhausted()
        }
    }
    /// The supplier's own reply, in the client's format.
    pub(crate) fn wire(status: u16, body: Value) -> Self {
        Self::Wire { status, body }
    }
    pub(crate) fn executor(error: ExecutorError) -> Self {
        match error {
            ExecutorError::NoSupplier { model_not_found } => Self::no_supplier(model_not_found),
            ExecutorError::Busy => Self::openai(
                StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
                "Every eligible supplier is busy. Please retry later.",
                Some("no_available_supplier"),
            ),
            ExecutorError::ContextRequired => crate::pool_scheduler::context_required(),
            ExecutorError::ConsumerInvalid => Self::invalid_token(),
            ExecutorError::Client { status, body, .. } => Self::wire(status, body),
            ExecutorError::Stopped { status, body, .. } => {
                let (status, body) = convert::escaped_reply(Inbound::CodexClient, status, &body);
                Self::wire(status.as_u16(), body)
            }
            ExecutorError::Convert(error) => Self::Caller(error),
            ExecutorError::Upstream(error) => Self::Upstream(error),
            ExecutorError::Storage(error) => Self::Storage(error),
        }
    }
}

/// Public API failure mapped to OpenAI-style `{ "error": { ... } }` JSON.
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error(transparent)]
    Service(#[from] crate::admission::ServiceError),
    #[error("{message}")]
    OpenAi {
        status: StatusCode,
        error_type: &'static str,
        message: String,
        code: Option<&'static str>,
    },
    #[error(transparent)]
    Upstream(#[from] UpstreamError),
    /// A supplier's reply the client receives as it is, in its own format.
    #[error("the supplier rejected the request with HTTP {status}")]
    Wire { status: u16, body: Value },
    /// A request the conversion matrix rejected before any supplier saw it.
    #[error(transparent)]
    Caller(#[from] convert::Error),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    SupplierAccount(#[from] AccountError),
}

impl OAuthError {
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

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        codex2api_upstream::RequestFailureLog::error(&self);
        match self {
            Self::Service(error) => service_error_response(error),
            Self::OpenAi {
                status,
                error_type,
                message,
                code,
            } => {
                let mut response = openai_response(status, error_type, message, code);
                if code == Some("supplier_concurrency_exceeded") {
                    response
                        .headers_mut()
                        .insert("retry-after", axum::http::HeaderValue::from_static("1"));
                }
                response
            }
            Self::Upstream(err) => upstream_error_response(err),
            Self::Wire { status, body } => {
                let (status, body) = convert::client_error(Inbound::CodexClient, status, &body);
                let mut response = (status, Json(body)).into_response();
                response.headers_mut().insert(
                    "cache-control",
                    axum::http::HeaderValue::from_static("no-store"),
                );
                response
            }
            Self::Caller(error) => (error.status, Json(error.body)).into_response(),
            Self::Storage(StorageError::AccountNotFound(_)) => {
                OAuthError::invalid_token().into_response()
            }
            Self::Storage(StorageError::Runtime) => openai_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
                "Request scheduling is temporarily unavailable.",
                Some("runtime_state_unavailable"),
            ),
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
                OAuthError::invalid_token().into_response()
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

pub fn upstream_error_response(err: UpstreamError) -> Response {
    if let UpstreamError::Status {
        status,
        body,
        mut headers,
        ..
    } = err
    {
        let value = serde_json::from_str::<Value>(&body).unwrap_or_default();
        let (status, error) = convert::escaped_reply(Inbound::CodexClient, status, &value);
        let mut response = (status, Json(error)).into_response();
        codex2api_upstream::strip_hop_by_hop_headers(&mut headers);
        for name in [
            "x-request-id",
            "x-oai-request-id",
            "cf-ray",
            "openai-model",
            "x-openai-model",
        ] {
            for value in headers.get_all(name) {
                response.headers_mut().append(name, value.clone());
            }
        }
        return response;
    }
    // Channel errors that are not a reply: this service's view of them. Supplier
    // credentials and their refresh are never the caller's concern.
    if matches!(
        err,
        UpstreamError::Refresh(_)
            | UpstreamError::Auth(_)
            | UpstreamError::GrokAuth(_)
            | UpstreamError::AntigravityAuth(_)
            | UpstreamError::ApiAuth(_)
            | UpstreamError::Unauthorized
            | UpstreamError::MissingAccessToken(_)
    ) {
        return crate::pool_execution::exhausted().into_response();
    }
    let failure = err.failure();
    let (status, kind, code, message) = match failure.kind {
        codex2api_upstream::FailureKind::InvalidRequest => (
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            failure
                .code
                .clone()
                .unwrap_or_else(|| "invalid_request_error".into()),
            failure
                .message
                .as_deref()
                .map(convert::sanitize::text)
                .unwrap_or_else(|| "The request is invalid.".into()),
        ),
        codex2api_upstream::FailureKind::Timeout => (
            StatusCode::GATEWAY_TIMEOUT,
            "server_error",
            "upstream_timeout".into(),
            "The upstream service timed out. Please try again later.".into(),
        ),
        codex2api_upstream::FailureKind::Authentication
        | codex2api_upstream::FailureKind::PaymentRequired
        | codex2api_upstream::FailureKind::Permission
        | codex2api_upstream::FailureKind::QuotaExhausted
        | codex2api_upstream::FailureKind::RateLimit
        | codex2api_upstream::FailureKind::LimitUnknown => {
            return crate::pool_execution::exhausted().into_response();
        }
        _ => (
            StatusCode::BAD_GATEWAY,
            "server_error",
            "upstream_error".into(),
            "The upstream service could not complete this request.".into(),
        ),
    };
    openai_response(status, kind, message, Some(&code))
}

fn service_error_response(error: crate::admission::ServiceError) -> Response {
    use crate::admission::ServiceError;
    use codex2api_common::PolicyError;
    let (status, kind, code) = match &error {
        ServiceError::RpmExceeded { retry_after } => {
            let mut response = openai_response(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_error",
                error.to_string(),
                Some("virtual_rpm_exceeded"),
            );
            if let Ok(value) = retry_after.to_string().parse() {
                response.headers_mut().insert("retry-after", value);
            }
            return response;
        }
        ServiceError::ConcurrencyExceeded => {
            let mut response = openai_response(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_error",
                error.to_string(),
                Some("virtual_concurrency_exceeded"),
            );
            response
                .headers_mut()
                .insert("retry-after", http::HeaderValue::from_static("1"));
            return response;
        }
        ServiceError::Storage(_) => {
            return match error {
                ServiceError::Storage(e) => OAuthError::Storage(e).into_response(),
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

    #[tokio::test]
    async fn refresh_errors_stay_private_and_keep_their_real_status_for_the_ledger() {
        for status in [400, 401, 403, 429, 503] {
            let error = UpstreamError::Refresh(codex2api_suppliers::AuthError::RefreshRejected {
                status,
                code: Some(
                    if status == 429 {
                        "rate_limit_exceeded"
                    } else {
                        "refresh_rejected"
                    }
                    .into(),
                ),
                message: "fixture refresh failure".into(),
            });
            let failure = error.failure();
            assert_eq!(failure.authentication_invalid(), status == 401);
            assert_eq!(failure.status, Some(status));
            assert_eq!(
                upstream_error_response(error).status(),
                StatusCode::SERVICE_UNAVAILABLE
            );
        }
    }

    #[test]
    fn server_errors_become_no_available_supplier() {
        let body = r#"{"error":{"message":"overloaded","type":"server_error"}}"#;
        let response =
            upstream_error_response(UpstreamError::status(StatusCode::SERVICE_UNAVAILABLE, body));
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn supplier_payment_errors_become_service_unavailable_without_details() {
        use axum::body::to_bytes;
        for body in [
            r#"{"detail":{"code":"deactivated_workspace","message":"private supplier billing details"}}"#,
            "Payment Required",
        ] {
            let response =
                upstream_error_response(UpstreamError::status(StatusCode::PAYMENT_REQUIRED, body));
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(value["error"]["code"], "no_available_supplier");
            let text = String::from_utf8_lossy(&bytes);
            assert!(!text.contains("private supplier") && !text.contains("deactivated_workspace"));
        }
    }

    #[tokio::test]
    async fn caller_errors_keep_their_reason_without_supplier_headers_or_auth_statuses() {
        use axum::body::to_bytes;
        use axum::http::{HeaderMap, HeaderValue};
        let body = json!({"error":{"code":"invalid_prompt", "message":"中文🦀".repeat(700)}});
        for status in [400, 401, 403, 426, 429, 503] {
            let mut headers = HeaderMap::new();
            for (key, value) in [
                ("x-error-json", r#"{"error":{"code":"invalid_prompt"}}"#),
                ("x-openai-authorization-error", "access_denied"),
                ("x-request-id", "request-fixture"),
                ("retry-after", "7"),
                ("connection", "x-oai-request-id"),
                ("x-oai-request-id", "connection-local"),
                ("set-cookie", "account=private"),
                ("authorization", "Bearer supplier-fixture"),
                ("x-codex-primary-used-percent", "65"),
                ("content-length", "999999"),
            ] {
                headers.insert(key, HeaderValue::from_static(value));
            }
            let response = upstream_error_response(UpstreamError::status_with_headers(
                StatusCode::from_u16(status).unwrap(),
                body.to_string(),
                headers,
            ));
            // Only a caller fault keeps its reason; a supplier's authorization,
            // throttling or server state is this service's exhaustion.
            let caller = !codex2api_upstream::supply::switch_status(status);
            assert_eq!(
                response.status(),
                if caller {
                    StatusCode::from_u16(status).unwrap()
                } else {
                    StatusCode::SERVICE_UNAVAILABLE
                },
                "{status}"
            );
            assert_eq!(response.headers()["x-request-id"], "request-fixture");
            for key in [
                "retry-after",
                "x-error-json",
                "x-openai-authorization-error",
                "x-oai-request-id",
                "set-cookie",
                "authorization",
                "x-codex-primary-used-percent",
                "content-length",
            ] {
                assert!(!response.headers().contains_key(key), "{key}");
            }
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let value = serde_json::from_slice::<Value>(&bytes).unwrap();
            if caller {
                assert_eq!(value["error"]["code"], "invalid_prompt");
                assert!(value["error"]["message"].as_str().unwrap().chars().count() <= 512);
            } else {
                assert_eq!(value["error"]["code"], "no_available_supplier");
                assert!(!String::from_utf8_lossy(&bytes).contains("invalid_prompt"));
            }
        }
    }
}

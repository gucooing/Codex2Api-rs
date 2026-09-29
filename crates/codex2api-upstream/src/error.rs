use thiserror::Error;

use codex2api_accounts::AccountError;
use codex2api_auth::AuthError;

pub type Result<T> = std::result::Result<T, UpstreamError>;

/// Errors talking to official Codex `/responses`.
#[derive(Debug, Error)]
pub enum UpstreamError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("request body exceeds the configured size limit")]
    RequestTooLarge,
    #[error("unsupported request Content-Encoding")]
    UnsupportedEncoding,
    #[error("request encoding failed: {0}")]
    Encoding(#[from] std::io::Error),
    #[error("account `{0}` has no access token")]
    MissingAccessToken(String),
    #[error("upstream returned 401 unauthorized")]
    Unauthorized,
    #[error("failed to refresh access token: {0}")]
    Refresh(#[source] AuthError),
    // The protocol body is kept intact; Display is deliberately content-free.
    // Callers record a separate bounded, redacted diagnostic in the usage ledger.
    #[error("upstream returned HTTP {status}")]
    Status {
        status: u16,
        body: String,
        headers: http::HeaderMap,
    },
    #[error("invalid header value: {0}")]
    InvalidHeader(#[from] http::header::InvalidHeaderValue),
    #[error("SSE stream error: {0}")]
    Stream(String),
    #[error("SSE stream idle timeout")]
    StreamIdleTimeout,
    #[error("supplier workspace routing failed: {0}")]
    WorkspaceRouting(String),
    #[error("supplier credentials or workspace routing changed; reconnect and retry")]
    WorkspaceChanged,
    #[error(transparent)]
    Storage(#[from] codex2api_storage::StorageError),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    SupplierAccount(#[from] AccountError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl UpstreamError {
    pub fn failure(&self) -> crate::ResponseFailure {
        use crate::ResponseFailure as F;
        if let Self::Status { status, body, .. } = self {
            return F::from_error(
                Some(*status),
                &serde_json::from_str(body).unwrap_or_default(),
            );
        }
        let (status, code, message) = match self {
            Self::Unauthorized => (401, "upstream_unauthorized", "上游授权失效"),
            Self::InvalidRequest(_) => (400, "invalid_request_error", "请求参数无效"),
            Self::RequestTooLarge => (413, "request_too_large", "请求超过大小限制"),
            Self::UnsupportedEncoding => (415, "unsupported_encoding", "请求编码不受支持"),
            Self::StreamIdleTimeout => (504, "upstream_timeout", "等待上游响应超时"),
            Self::Http(e) if e.is_timeout() => (504, "upstream_timeout", "上游请求超时"),
            Self::Http(e) if e.is_connect() => {
                (502, "upstream_connection_failed", "无法连接上游服务")
            }
            Self::Http(_) => (502, "upstream_transport_error", "上游传输失败"),
            Self::Stream(message) => {
                return F::new(Some(502), Some("upstream_stream_error"), Some(message));
            }
            Self::Refresh(e) | Self::Auth(e) => match e {
                AuthError::RefreshRejected {
                    status,
                    code,
                    message,
                } => return F::new(Some(*status), code.as_deref(), Some(message)),
                AuthError::TokenEndpoint { status, message } => {
                    return F::new(Some(*status), None, Some(message));
                }
                AuthError::Http(e) if e.is_timeout() => {
                    (504, "upstream_timeout", "授权服务请求超时")
                }
                AuthError::Http(_) | AuthError::Io(_) => {
                    (502, "upstream_transport_error", "授权服务连接失败")
                }
                _ => (503, "upstream_auth_unavailable", "上游授权当前不可用"),
            },
            Self::MissingAccessToken(_) => (503, "upstream_auth_missing", "供应账户没有可用授权"),
            Self::WorkspaceChanged => (
                409,
                "supplier_workspace_changed",
                "供应授权或路由已变化，请重新连接",
            ),
            _ => (502, "upstream_error", "上游请求未能完成"),
        };
        F::new(Some(status), Some(code), Some(message))
    }

    pub fn status(status: reqwest::StatusCode, body: impl Into<String>) -> Self {
        Self::status_with_headers(status, body, http::HeaderMap::new())
    }

    pub fn status_with_headers(
        status: reqwest::StatusCode,
        body: impl Into<String>,
        headers: http::HeaderMap,
    ) -> Self {
        Self::Status {
            status: status.as_u16(),
            body: body.into(),
            headers,
        }
    }

    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized | Self::Status { status: 401, .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_preserves_long_unicode_json_and_classifies_unauthorized_without_logging_content() {
        let body = serde_json::json!({"error":{
            "code":"invalid_prompt", "message":"中文🦀".repeat(700)
        }})
        .to_string();
        for status in [
            reqwest::StatusCode::BAD_REQUEST,
            reqwest::StatusCode::UNAUTHORIZED,
        ] {
            let error = UpstreamError::status(status, body.clone());
            assert_eq!(
                error.is_unauthorized(),
                status == reqwest::StatusCode::UNAUTHORIZED
            );
            assert_eq!(
                error.to_string(),
                format!("upstream returned HTTP {}", status.as_u16())
            );
            let UpstreamError::Status {
                body: preserved, ..
            } = error
            else {
                panic!("missing HTTP error")
            };
            assert_eq!(preserved, body);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&preserved).unwrap()["error"]["code"],
                "invalid_prompt"
            );
        }
    }
}

use axum::{http::StatusCode, response::Response};

pub async fn unavailable() -> crate::Result<Response> {
    Err(crate::OAuthError::openai(
        StatusCode::NOT_IMPLEMENTED,
        "invalid_request_error",
        "Voice calls are not supported by this service.",
        Some("voice_calls_unavailable"),
    ))
}

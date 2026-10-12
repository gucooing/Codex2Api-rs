use axum::extract::{Extension, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::{ApiState, Result};
use codex2api_upstream::{Endpoint, normalize_response_identity, serialize_responses_request};

pub async fn responses_websocket(
    _: crate::user_agent::AllowedUserAgent,
    State(state): State<ApiState>,
    Extension(endpoint): Extension<Endpoint>,
    oauth: Extension<codex2api_storage::VirtualAccess>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    super::pooled_websocket::connect(state, endpoint, oauth.0, headers, upgrade).await
}

#[cfg(test)]
pub(super) fn prepare_message(
    text: &str,
    installation_id: &str,
    timezone: Option<&str>,
    guardian_reviewer: bool,
) -> codex2api_upstream::Result<String> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| codex2api_upstream::UpstreamError::InvalidRequest(e.to_string()))?;
    prepare_value(value, installation_id, timezone, guardian_reviewer)
}

pub(super) fn prepare_value(
    mut value: serde_json::Value,
    installation_id: &str,
    timezone: Option<&str>,
    guardian_reviewer: bool,
) -> codex2api_upstream::Result<String> {
    if value.get("type").and_then(serde_json::Value::as_str) == Some("response.create") {
        codex2api_upstream::apply_response_timezone(&mut value, timezone)?;
        let mut headers = HeaderMap::new();
        if guardian_reviewer {
            headers.insert(
                "x-codex-guardian",
                axum::http::HeaderValue::from_static("reviewer"),
            );
        }
        normalize_response_identity(&mut value, installation_id, &headers)?;
        return serialize_responses_request(&value);
    }
    Ok(serde_json::to_string(&value)?)
}

pub(super) fn relay_error(error: impl std::fmt::Display) -> crate::ApiError {
    tracing::debug!(%error, "WebSocket transport or frame error");
    crate::ApiError::openai(
        axum::http::StatusCode::BAD_GATEWAY,
        "server_error",
        "The Responses connection ended before completion.",
        Some("stream_error"),
    )
}

pub(super) async fn error_message(error: crate::ApiError) -> (u16, String) {
    let response = error.into_response();
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap_or_default();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        serde_json::json!({"error":{"type":"server_error","message":"Responses relay failed."}})
    });
    value["type"] = "error".into();
    value["status"] = status.into();
    if let Some(retry_after) = retry_after {
        value["headers"] = serde_json::json!({"retry-after":retry_after});
    }
    (status, value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_prefix_preserves_warmup_incremental_input_and_controls() {
        let input = r#"{"input":[],"previous_response_id":"previous","generate":false,"access_programs":{"cyber":"daybreak_blue"},"model":"test","service_tier":"priority","stream":true,"type":"response.create"}"#;
        for reviewer in [false, true] {
            let text = prepare_message(input, "installation", None, reviewer).unwrap();
            let prefix = if reviewer {
                r#"{"type":"response.create","model":"test","stream":true,"input":[]"#
            } else {
                r#"{"type":"response.create","model":"test","stream":true,"service_tier":"priority","input":[]"#
            };
            assert!(text.starts_with(prefix), "{text}");
            let mut expected: serde_json::Value = serde_json::from_str(input).unwrap();
            if reviewer {
                expected.as_object_mut().unwrap().remove("service_tier");
            }
            expected["client_metadata"] =
                serde_json::json!({"x-codex-installation-id":"installation"});
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&text).unwrap(),
                expected
            );
        }
        for control in [
            r#"{"response_id":"active","type":"response.interrupt"}"#,
            r#"{"future_field":true,"type":"future.control"}"#,
        ] {
            assert_eq!(
                prepare_message(control, "installation", None, false).unwrap(),
                control
            );
        }
    }
}

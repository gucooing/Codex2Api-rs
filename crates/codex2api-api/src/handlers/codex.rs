use crate::response::forward_response;
use axum::body::{Body, Bytes};
use axum::extract::Extension;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;

use crate::auth::authenticate;
use crate::error::Result;
use crate::state::ApiState;

/// Rebuild official request identity while preserving the conversation and response stream.
pub async fn forward(
    State(state): State<ApiState>,
    Extension(endpoint): Extension<codex2api_upstream::Endpoint>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let start = std::time::Instant::now();
    let requested_at = chrono::Utc::now().timestamp_millis();
    let (key, ctx) = authenticate(&state, &headers).await?;
    let mut log = if crate::usage::billable(endpoint) {
        let bytes = body.clone();
        let inbound = headers.clone();
        let metadata = tokio::task::spawn_blocking(move || {
            codex2api_upstream::request_metadata(&bytes, &inbound)
        })
        .await
        .map_err(|e| crate::ApiError::internal(e.to_string()))??;
        Some(
            crate::usage::UsageContext::new(
                state.storage.clone(),
                &ctx.account,
                &key,
                &format!("/v1/{}", endpoint.codex_path()),
                "http",
            )
            .start(metadata, start, requested_at)
            .await?,
        )
    } else {
        None
    };
    let result = async {
        let upstream = state.upstream.get(&ctx.account.id).await?;
        upstream.forward_endpoint(endpoint, body, headers).await
    }
    .await;
    let response = match result {
        Ok(response) => response,
        Err(error) => {
            if let Some(log) = &mut log {
                log.finish("failed");
            }
            return Err(error.into());
        }
    };
    let status = response.status();
    let headers = response.headers().clone();
    let body = if let Some(mut log) = log {
        log.http_status(status.as_u16());
        log.wrap(response)
    } else {
        Body::from_stream(response.bytes_stream())
    };
    Ok(forward_response(status, headers, body))
}

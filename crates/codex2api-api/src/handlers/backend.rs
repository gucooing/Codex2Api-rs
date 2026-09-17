use crate::{ApiState, Result};
use axum::body::{Body, Bytes};
use axum::extract::{Extension, OriginalUri, Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use codex2api_upstream::BackendEndpoint;
use std::collections::HashMap;

pub async fn forward(
    State(state): State<ApiState>,
    Extension(endpoint): Extension<BackendEndpoint>,
    Path(parameters): Path<HashMap<String, String>>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let (_, ctx) = crate::auth::authenticate(&state, &headers).await?;
    let upstream = state.upstream.get(&ctx.account.id).await?;
    let response = upstream
        .forward_backend(endpoint, &parameters, uri.query(), body, headers)
        .await?;
    let status = response.status();
    let headers = response.headers().clone();
    Ok(crate::response::forward_response(
        status,
        headers,
        Body::from_stream(response.bytes_stream()),
    ))
}

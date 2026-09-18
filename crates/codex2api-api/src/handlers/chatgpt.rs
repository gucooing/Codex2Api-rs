use crate::{ApiState, Result};
use axum::{
    body::{Body, Bytes},
    extract::{Extension, OriginalUri, State},
    http::HeaderMap,
    response::Response,
};
use codex2api_storage::OAuthAccess;
use codex2api_upstream::ChatgptEndpoint;

pub async fn forward(
    State(state): State<ApiState>,
    Extension(endpoint): Extension<ChatgptEndpoint>,
    oauth: Option<Extension<OAuthAccess>>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let (_, ctx) = crate::auth::authenticate_request(&state, &headers, oauth).await?;
    let account_id = ctx.account.chatgpt_account_id.as_deref().unwrap_or("");
    endpoint.url(account_id, uri.query())?;
    let upstream = state.upstream.get(&ctx.account.id).await?;
    let response = upstream
        .forward_chatgpt(endpoint, account_id, uri.query(), body, headers)
        .await?;
    Ok(crate::response::forward_response(
        response.status(),
        response.headers().clone(),
        Body::from_stream(response.bytes_stream()),
    ))
}

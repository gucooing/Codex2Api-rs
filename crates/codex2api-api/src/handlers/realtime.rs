use crate::{ApiState, Result};
use axum::body::{Body, Bytes};
use axum::extract::{Extension, OriginalUri, Path, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use codex2api_upstream::{RealtimeKind, strip_hop_by_hop_headers};
use std::collections::HashMap;

pub async fn call(
    _: crate::user_agent::AllowedUserAgent,
    State(state): State<ApiState>,
    Extension(kind): Extension<RealtimeKind>,
    oauth: Option<Extension<codex2api_storage::OAuthAccess>>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let (_, ctx) = crate::auth::authenticate_request(&state, &headers, oauth).await?;
    let client = state.upstream.get(&ctx.account.id).await?;
    let response = client
        .forward_realtime_call(kind, uri.query(), body, headers)
        .await?;
    Ok(crate::response::forward_response(
        response.status(),
        response.headers().clone(),
        Body::from_stream(response.bytes_stream()),
    ))
}

pub async fn socket(
    _: crate::user_agent::AllowedUserAgent,
    State(state): State<ApiState>,
    Extension(kind): Extension<RealtimeKind>,
    oauth: Option<Extension<codex2api_storage::OAuthAccess>>,
    Path(parameters): Path<HashMap<String, String>>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    let (key, ctx) = crate::auth::authenticate_request(&state, &headers, oauth).await?;
    let client = state.upstream.get(&ctx.account.id).await?;
    let (upstream, mut headers) = client
        .connect_realtime(
            kind,
            parameters.get("call_id").map(String::as_str),
            uri.query(),
            headers,
        )
        .await?;
    strip_hop_by_hop_headers(&mut headers);
    for name in [
        "sec-websocket-accept",
        "sec-websocket-extensions",
        "sec-websocket-protocol",
        "set-cookie",
        "content-length",
    ] {
        headers.remove(name);
    }
    let identity = client.identity().installation_id.clone();
    let mut response = upgrade
        .max_message_size(codex2api_upstream::MAX_REQUEST_BYTES)
        .on_upgrade(move |socket| {
            super::websocket::bridge(
                socket,
                upstream,
                identity,
                true,
                Some((state.storage, key.access)),
            )
        });
    response.headers_mut().extend(headers);
    Ok(response)
}

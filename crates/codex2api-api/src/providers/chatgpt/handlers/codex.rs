use crate::response::forward_response;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::extract::{Extension, Path};
use axum::http::HeaderMap;
use axum::response::Response;
use futures::StreamExt;

use crate::error::Result;
use crate::providers::chatgpt::access::resolve_supplier_for;
use crate::state::ApiState;

/// Rebuild official request identity while preserving the conversation and response stream.
pub async fn forward(
    _: crate::user_agent::AllowedUserAgent,
    State(state): State<ApiState>,
    Extension(endpoint): Extension<codex2api_upstream::Endpoint>,
    Path(parameters): Path<std::collections::HashMap<String, String>>,
    oauth: Extension<codex2api_storage::VirtualAccess>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    crate::response::complete_on_disconnect(async move {
        let virtual_id = oauth.virtual_account_id.clone();
        if endpoint == codex2api_upstream::Endpoint::Models {
            return Ok(crate::providers::chatgpt::identity::json_response(
                crate::providers::chatgpt::identity::codex_model_catalog(
                    &state.storage,
                    &virtual_id,
                )
                .await?,
            ));
        }
        if crate::usage::billable(endpoint) {
            crate::providers::chatgpt::access::check_virtual_quota(&state.storage, &virtual_id)
                .await?;
        }
        let start = std::time::Instant::now();
        let requested_at = chrono::Utc::now().timestamp_millis();
        let subpath = parameters.get("subpath");
        if let Some(subpath) = subpath {
            codex2api_upstream::responses_subpath_url(subpath)?;
        }
        let path = subpath
            .map(|s| format!("responses/{s}"))
            .unwrap_or_else(|| endpoint.codex_path().into());
        let bytes = body.clone();
        let inbound = headers.clone();
        let (schedule, metadata) = tokio::task::spawn_blocking(move || {
            let value = codex2api_upstream::decode_body(&bytes, &inbound)?;
            Ok::<_, codex2api_upstream::UpstreamError>((
                crate::providers::chatgpt::scheduling::request(&inbound, &value),
                codex2api_upstream::request_metadata_value(&value),
            ))
        })
        .await
        .map_err(|e| crate::ApiError::internal(e.to_string()))??;
        let (key, mut ctx) =
            resolve_supplier_for(&state, &headers, oauth.clone(), &schedule).await?;
        if endpoint == codex2api_upstream::Endpoint::InputTokens {
            crate::execution::ExecutionContext::new(
                state.storage.clone(),
                &ctx.account,
                &key.id,
                &key.name,
                "/v1/responses/input_tokens",
                "http",
            )
            .authorize(&metadata, true)
            .await?;
        }
        let mut log = if crate::usage::billable(endpoint) {
            Some(
                crate::execution::ExecutionContext::new(
                    state.storage.clone(),
                    &ctx.account,
                    &key.id,
                    &key.name,
                    &format!("/v1/{path}"),
                    "http",
                )
                .with_supplier_permit(ctx.permit.take())
                .start(metadata, start, requested_at)
                .await?,
            )
        } else {
            None
        };
        let mut response = crate::providers::chatgpt::execution::forward(
            &state,
            &oauth,
            ctx,
            endpoint,
            subpath.map(String::as_str),
            body,
            headers,
            &mut log,
        )
        .await?;
        let status = response.status();
        let mut headers = response.headers().clone();
        crate::providers::chatgpt::identity::quota_headers(
            &state.storage,
            &virtual_id,
            &mut headers,
        )
        .await?;
        let isolate = |body| {
            if headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|s| s.starts_with("text/event-stream"))
            {
                crate::providers::chatgpt::identity::isolate_sse(body, state.storage, virtual_id)
            } else {
                body
            }
        };
        let body = if let Some(mut log) = log {
            log.http_status(status.as_u16());
            log.wrap_with(response, isolate)
        } else {
            let permit = response
                .extensions_mut()
                .remove::<codex2api_storage::RuntimePermit>();
            let upstream = response.bytes_stream();
            isolate(Body::from_stream(futures::stream::unfold(
                (upstream, permit),
                |(mut stream, permit)| async move {
                    stream.next().await.map(|chunk| (chunk, (stream, permit)))
                },
            )))
        };
        Ok(forward_response(status, headers, body))
    })
    .await
}

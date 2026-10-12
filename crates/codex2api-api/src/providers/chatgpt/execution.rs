//! ChatGPT endpoint dispatch using the shared supplier pool.
use crate::{
    ApiState, Result,
    pool_execution::{SupplierContext, execute},
};
use axum::body::Bytes;
use codex2api_storage::VirtualAccess;
use codex2api_upstream::Endpoint;
use http::HeaderMap;
pub(crate) async fn forward(
    state: &ApiState,
    oauth: &VirtualAccess,
    ctx: SupplierContext,
    endpoint: Endpoint,
    subpath: Option<&str>,
    body: Bytes,
    decoded: serde_json::Value,
    headers: HeaderMap,
    log: &mut Option<crate::usage::RequestLog>,
) -> Result<reqwest::Response> {
    let original = ctx.account.id.clone();
    let reused_affinity = ctx.reused_affinity;
    let mut decoded = Some(decoded);
    execute(state, oauth, ctx, log, |id: String| {
        let body = body.clone();
        let decoded = decoded.take();
        let mut headers = headers.clone();
        if !reused_affinity || id != original {
            headers.remove("x-codex-turn-state");
        }
        async move {
            let upstream = state.upstream.get(&id).await?;
            if let Some(value) = decoded {
                return upstream
                    .forward_decoded_endpoint(endpoint, subpath, value, headers)
                    .await;
            }
            if let Some(subpath) = subpath {
                upstream
                    .forward_responses_subpath(subpath, body, headers)
                    .await
            } else {
                upstream.forward_endpoint(endpoint, body, headers).await
            }
        }
    })
    .await
}

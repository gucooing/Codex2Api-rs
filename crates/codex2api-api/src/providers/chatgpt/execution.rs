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
    headers: HeaderMap,
    log: &mut Option<crate::usage::RequestLog>,
) -> Result<reqwest::Response> {
    execute(state, oauth, ctx, log, |id: String| {
        let body = body.clone();
        let mut headers = headers.clone();
        if oauth.account_id.as_deref() != Some(id.as_str()) {
            headers.remove("x-codex-turn-state");
        }
        async move {
            let upstream = state.upstream.get(&id).await?;
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

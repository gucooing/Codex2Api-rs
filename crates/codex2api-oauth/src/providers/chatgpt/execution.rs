//! ChatGPT endpoint dispatch through the shared executor: the official client's
//! request stays in its own format and is bound to the selected supplier's identity
//! by the ChatGPT channel.
use crate::{
    OAuthState, Result,
    pool_execution::{SupplierContext, execute},
};
use codex2api_storage::VirtualAccess;
use codex2api_upstream::{
    Endpoint,
    executor::Executed,
    inbound::{Inbound, InboundRequest},
    supply::Operation,
};
use http::HeaderMap;

/// The operation an official Codex endpoint performs.
pub(crate) fn operation(endpoint: Endpoint) -> Operation {
    match endpoint {
        Endpoint::Compact => Operation::Compact,
        Endpoint::InputTokens => Operation::InputTokens,
        Endpoint::Search => Operation::Search,
        Endpoint::ImageGeneration => Operation::ImageGeneration,
        Endpoint::ImageEdit => Operation::ImageEdit,
        _ => Operation::Generate,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn forward(
    state: &OAuthState,
    oauth: &VirtualAccess,
    ctx: SupplierContext,
    endpoint: Endpoint,
    subpath: Option<&str>,
    decoded: serde_json::Value,
    headers: HeaderMap,
    log: &mut Option<crate::usage::RequestLog>,
) -> Result<Executed> {
    let mut headers = headers;
    if !ctx.reused_affinity {
        headers.remove("x-codex-turn-state");
    }
    let path = match subpath {
        Some(subpath) => format!("responses/{subpath}"),
        None => endpoint.codex_path().to_owned(),
    };
    let input = InboundRequest::client(
        Inbound::CodexClient,
        operation(endpoint),
        decoded,
        headers,
        Some(path),
    );
    execute(state, oauth, ctx, input, log).await
}

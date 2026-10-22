//! API consumer dispatch: the consumer's route into the executor. Supplier
//! selection, conversion, verdicts and switching happen in the executor.
use crate::{ApiError, ApiState, InboundRequest, Result, usage::ApiLog};
use codex2api_storage::ApiPrincipal;
use codex2api_upstream::{
    executor::{Executed, Execution},
    scheduler::{SupplierContext, SupplierRoute},
    supply::{Operation, Wire},
};
use http::HeaderMap;

pub(crate) async fn validate(
    state: &ApiState,
    principal: &ApiPrincipal,
    token: &str,
) -> codex2api_upstream::scheduler::Result<()> {
    let current = state
        .storage
        .authenticate_api_key(token)
        .await?
        .ok_or(codex2api_upstream::scheduler::SchedulerError::ConsumerInvalid)?;
    if current.user_id != principal.user_id
        || current.key_id != principal.key_id
        || current.group.id != principal.group.id
        || current.group.revision != principal.group.revision
    {
        return Err(codex2api_upstream::scheduler::SchedulerError::ConsumerInvalid);
    }
    Ok(())
}

/// `provider` pins the supplier platform for protocol-specific operations; a
/// generation may run on any platform whose tagged supplier serves the model.
pub(crate) fn route(principal: &ApiPrincipal, provider: Option<&str>) -> SupplierRoute {
    SupplierRoute {
        owner_id: format!(
            "api:{}:{}:{}",
            principal.user_id, principal.key_id, principal.group.id
        ),
        scope: "api".into(),
        provider_id: provider.map(str::to_owned),
        tag_ids: principal.group.tag_ids.clone(),
    }
}

/// The supplier that minted a Codex turn state, if this caller received one from it.
pub(crate) async fn turn_owner(
    state: &ApiState,
    route: &SupplierRoute,
    inbound: &HeaderMap,
) -> codex2api_upstream::Result<Option<String>> {
    let Some(turn) = inbound.get("x-codex-turn-state") else {
        return Ok(None);
    };
    let key = codex2api_storage::RoutingKey::turn(
        &route.owner_id,
        &route.scope,
        turn.to_str().unwrap_or(""),
    );
    Ok(state.storage.routing_affinity(&key).await?)
}

/// Turn state is an opaque supplier capability. Only replay a state that this
/// caller received from the selected supplier; never derive new conversation IDs.
pub(crate) async fn owned_headers(
    state: &ApiState,
    route: &SupplierRoute,
    context: &SupplierContext,
    inbound: &HeaderMap,
) -> codex2api_upstream::Result<HeaderMap> {
    let mut headers = inbound.clone();
    if turn_owner(state, route, inbound).await?.as_deref() != Some(context.account.id.as_str()) {
        headers.remove("x-codex-turn-state");
    }
    Ok(headers)
}

pub(crate) async fn remember_affinity(
    state: &ApiState,
    route: &SupplierRoute,
    account: &str,
    response_id: Option<&str>,
    headers: Option<&HeaderMap>,
) -> Result<()> {
    let response = response_id
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .map(|id| codex2api_storage::RoutingKey::response(&route.owner_id, &route.scope, id));
    let turn = headers
        .and_then(|h| h.get("x-codex-turn-state"))
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= 16384)
        .map(|v| codex2api_storage::RoutingKey::turn(&route.owner_id, &route.scope, v));
    let bindings: Vec<_> = response
        .iter()
        .chain(turn.iter())
        .map(|key| (key, account))
        .collect();
    if bindings.is_empty() {
        return Ok(());
    }
    let ttl = state.storage.gateway_settings().await?.sticky_ttl_seconds;
    state
        .storage
        .save_routing_affinities(&bindings, ttl)
        .await?;
    Ok(())
}

/// Operations that exist only on the ChatGPT channel pin the route to it.
fn pinned_provider(operation: Operation) -> Option<&'static str> {
    match operation {
        Operation::Compact | Operation::InputTokens | Operation::Search => {
            Some(codex2api_common::CHATGPT)
        }
        _ => None,
    }
}

pub(crate) async fn forward(
    state: &ApiState,
    principal: &ApiPrincipal,
    token: &str,
    input: &InboundRequest,
    log: &mut ApiLog,
) -> Result<(Executed, SupplierRoute)> {
    let provider = pinned_provider(input.operation);
    let route = route(principal, provider);
    let schedule = codex2api_upstream::executor::schedule(
        provider.map(|_| Wire::Codex),
        &input.headers,
        &input.body,
    );
    let owner = turn_owner(state, &route, &input.headers).await?;
    let execution = Execution {
        storage: &state.storage,
        pool: &state.upstream,
        route: &route,
        schedule,
        inbound: input,
        ledger: log,
        excluded: Vec::new(),
        headers_for: Some(Box::new(move |account, headers| {
            if owner.as_deref() != Some(account.id.as_str()) {
                headers.remove("x-codex-turn-state");
            }
        })),
        initial: None,
    };
    let executed = execution
        .run(|| validate(state, principal, token))
        .await
        .map_err(|error| ApiError::executor(error, input.format))?;
    Ok((executed, route))
}

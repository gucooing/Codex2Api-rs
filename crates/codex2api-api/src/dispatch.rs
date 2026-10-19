//! API consumer dispatch; supplier selection precedes upstream protocol construction.
use crate::{ApiState, InboundRequest, PreparedRequest, Result, usage::ApiLog};
use codex2api_storage::ApiPrincipal;
use codex2api_upstream::{
    SupplierFailure, UpstreamError,
    scheduler::{SupplierContext, SupplierRoute},
};
use http::{HeaderMap, StatusCode};
use serde_json::Value;
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

pub(crate) fn route(principal: &ApiPrincipal) -> SupplierRoute {
    SupplierRoute {
        owner_id: format!(
            "api:{}:{}:{}",
            principal.user_id, principal.key_id, principal.group.id
        ),
        provider_id: principal.group.provider_id.clone(),
        tag_ids: principal.group.tag_ids.clone(),
    }
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
    if let Some(turn) = headers.get("x-codex-turn-state") {
        let key = codex2api_storage::RoutingKey::turn(
            &route.owner_id,
            &route.provider_id,
            turn.to_str().unwrap_or(""),
        );
        if state.storage.routing_affinity(&key).await?.as_deref()
            != Some(context.account.id.as_str())
        {
            headers.remove("x-codex-turn-state");
        }
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
    let ttl = state.storage.gateway_settings().await?.sticky_ttl_seconds;
    let response = response_id
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .map(|id| codex2api_storage::RoutingKey::response(&route.owner_id, &route.provider_id, id));
    let turn = headers
        .and_then(|h| h.get("x-codex-turn-state"))
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= 16384)
        .map(|v| codex2api_storage::RoutingKey::turn(&route.owner_id, &route.provider_id, v));
    let bindings: Vec<_> = response
        .iter()
        .chain(turn.iter())
        .map(|key| (key, account))
        .collect();
    state
        .storage
        .save_routing_affinities(&bindings, ttl)
        .await?;
    Ok(())
}

pub(crate) async fn forward(
    state: &ApiState,
    principal: &ApiPrincipal,
    token: &str,
    input: &InboundRequest,
    log: &mut ApiLog,
) -> Result<(
    reqwest::Response,
    SupplierContext,
    SupplierRoute,
    PreparedRequest,
)> {
    let route = route(principal);
    let schedule =
        codex2api_upstream::protocol::scheduling(&route.provider_id, &input.headers, &input.body)?;
    let mut excluded = vec![];
    let mut refreshed = std::collections::HashSet::new();
    loop {
        validate(state, principal, token).await?;
        let context = codex2api_upstream::scheduler::select(
            &state.storage,
            &route,
            &schedule,
            &excluded,
            || validate(state, principal, token),
        )
        .await?;
        log.record.account_id = context.account.id.clone();
        log.record.account_name = context
            .account
            .display_name
            .clone()
            .unwrap_or_else(|| context.account.email.clone().unwrap_or_default());
        // A refresh retries this supplier once without releasing its reserved slot.
        loop {
            // Each attempt is built for the selected account; its body moves into the transport.
            let mut prepared =
                codex2api_upstream::protocol::prepare_for(&context.account.provider_id, input)?;
            let mut revision = state
                .storage
                .supplier_auth_revision(&context.account.id)
                .await?;
            log.record.http_status = None;
            let result = async {
                let inbound = owned_headers(state, &route, &context, &prepared.headers).await?;
                let endpoint = prepared.endpoint();
                state
                    .storage
                    .set_usage_upstream_endpoint(&log.record.id, endpoint)
                    .await?;
                log.record.upstream_endpoint = Some(endpoint.into());
                let response = codex2api_upstream::protocol::send(
                    &state.upstream,
                    &context.account,
                    &inbound,
                    prepared.upstream,
                    std::mem::take(&mut prepared.body),
                )
                .await?;
                revision = response
                    .extensions()
                    .get::<codex2api_upstream::SupplierAuthRevision>()
                    .map(|r| r.0)
                    .or(revision);
                codex2api_upstream::scheduler::inspect_response(
                    &route.provider_id,
                    codex2api_upstream::providers::prelude(&route.provider_id),
                    response,
                )
                .await
            }
            .await;
            match result {
                Ok(response) => {
                    log.auth_revision = revision;
                    return Ok((response, context, route, prepared));
                }
                Err(error) => {
                    if let UpstreamError::Status {
                        status,
                        body,
                        headers,
                        ..
                    } = &error
                    {
                        log.record.http_status = Some(i64::from(*status));
                        if let Ok(value) = serde_json::from_str::<Value>(body) {
                            if codex2api_upstream::scheduler::generation_observed(&value) {
                                log.headers(
                                    StatusCode::from_u16(*status)
                                        .unwrap_or(StatusCode::BAD_GATEWAY),
                                    headers,
                                );
                                log.measure(&value, prepared.upstream);
                                if let Some(failure) = error.supplier_failure_for(
                                    &route.provider_id,
                                    chrono::Utc::now().timestamp(),
                                ) {
                                    codex2api_upstream::scheduler::observe_failure(
                                        &state.storage,
                                        &context.account.id,
                                        error.auth_revision().or(revision),
                                        &failure,
                                    )
                                    .await?;
                                }
                                // Output or usage proves this attempt ran. Never duplicate it on
                                // another supplier or refresh and execute the request again.
                                log.fail_upstream(&error);
                                return Err(error.into());
                            }
                        }
                    }
                    revision = error.auth_revision().or(revision);
                    let Some(mut failure) = error
                        .supplier_failure_for(&route.provider_id, chrono::Utc::now().timestamp())
                    else {
                        log.fail_upstream(&error);
                        return Err(error.into());
                    };
                    if failure == SupplierFailure::Authentication
                        && refreshed.insert(context.account.id.clone())
                    {
                        match codex2api_upstream::scheduler::recover_auth(
                            &state.storage,
                            &state.upstream,
                            &context,
                            revision,
                        )
                        .await?
                        {
                            None => continue,
                            Some(rejection) => failure = rejection,
                        }
                    }
                    codex2api_upstream::scheduler::observe_failure(
                        &state.storage,
                        &context.account.id,
                        revision,
                        &failure,
                    )
                    .await?;
                    state
                        .storage
                        .report_supplier_result(&context.account.id, false, None);
                    excluded.push(context.account.id.clone());
                    break;
                }
            }
        }
    }
}

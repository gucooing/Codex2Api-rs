//! Provider-neutral pool scheduling. Providers construct their own affinity seeds.
use crate::{ApiError, ApiState, Result, pool_execution::SupplierContext};
use codex2api_storage::VirtualAccess;
use serde_json::Value;
use sha2::{Digest, Sha256};

#[cfg(test)]
#[path = "pool_scheduler_tests.rs"]
mod tests;

#[derive(Clone, Debug, Default)]
pub(crate) struct ScheduleRequest {
    pub model: Option<String>,
    pub session_hash: Option<String>,
    pub previous_response: Option<String>,
    pub previous_can_move: bool,
    pub admission: Option<(u64, u64, i64)>,
}
pub(crate) fn hash(seed: &str) -> String {
    format!("{:x}", Sha256::digest(seed.as_bytes()))
}
pub(crate) fn header_seed(headers: &http::HeaderMap) -> Option<String> {
    [
        "session-id",
        "session_id",
        "conversation_id",
        "x-session-affinity",
        "x-session-id",
        "x-opencode-session",
        "x-conversation-id",
    ]
    .iter()
    .find_map(|name| {
        headers
            .get(*name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    })
}
pub(crate) fn body_seed(body: &Value) -> Option<String> {
    let input = body.get("input").or_else(|| body.get("messages"));
    let first = match input {
        Some(Value::Array(items)) => items.iter().find(|item| item["role"] == "user"),
        other => other,
    };
    let first = first
        .filter(|value| !value.is_null() && value.as_str().is_none_or(|s| !s.trim().is_empty()))?;
    let system_messages: Vec<_> = input
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| matches!(item["role"].as_str(), Some("system" | "developer")))
        .collect();
    serde_json::to_string(&(
        body.get("model"),
        body.get("instructions").or_else(|| body.get("system")),
        system_messages,
        body.get("tools"),
        first,
    ))
    .ok()
}
pub(crate) fn text(body: &Value, key: &str) -> Option<String> {
    body.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}
pub(crate) fn previous_key(owner: &str, provider: &str, response: &str) -> String {
    format!(
        "response_owner:{}",
        hash(&serde_json::json!([owner, provider, response]).to_string())
    )
}
fn busy() -> ApiError {
    ApiError::openai(
        http::StatusCode::TOO_MANY_REQUESTS,
        "rate_limit_error",
        "All eligible supplier accounts are at their concurrency limit.",
        Some("supplier_concurrency_exceeded"),
    )
}
pub(crate) fn context_required() -> ApiError {
    ApiError::openai(
        http::StatusCode::CONFLICT,
        "invalid_request_error",
        "The previous response cannot be continued on an available supplier. Send the full conversation without previous_response_id.",
        Some("supplier_context_required"),
    )
}
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(2685821657736338717)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub(crate) async fn select(
    state: &ApiState,
    oauth: &VirtualAccess,
    request: &ScheduleRequest,
    excluded: &[String],
) -> Result<SupplierContext> {
    let route = state
        .storage
        .execution_route(&oauth.virtual_account_id, &oauth.provider_id)
        .await?
        .ok_or_else(crate::pool_execution::exhausted)?;
    let tag = route.tag_id.ok_or_else(crate::pool_execution::exhausted)?;
    let settings = state.storage.gateway_settings().await?;
    let sticky_key = request.session_hash.as_ref().map(|hash| {
        format!(
            "sticky_session:{}:{tag}:{}:{hash}",
            oauth.provider_id, oauth.virtual_account_id
        )
    });
    let sticky = match &sticky_key {
        Some(key) => state.storage.routing_affinity(key).await?,
        None => None,
    };
    let previous = match &request.previous_response {
        Some(id) => {
            state
                .storage
                .routing_affinity(&previous_key(
                    &oauth.virtual_account_id,
                    &oauth.provider_id,
                    id,
                ))
                .await?
        }
        None => None,
    };
    let previous_candidate = match previous
        .as_deref()
        .filter(|id| !excluded.iter().any(|v| v == id))
    {
        Some(id) => {
            state
                .storage
                .pool_candidate(&tag, &oauth.provider_id, request.model.as_deref(), id)
                .await?
        }
        None => None,
    };
    let sticky_candidate = if sticky == previous {
        previous_candidate.clone()
    } else {
        match sticky
            .as_deref()
            .filter(|id| !excluded.iter().any(|v| v == id))
        {
            Some(id) => {
                state
                    .storage
                    .pool_candidate(&tag, &oauth.provider_id, request.model.as_deref(), id)
                    .await?
            }
            None => None,
        }
    };
    if let (Some(key), Some(id)) = (&sticky_key, &sticky)
        && sticky_candidate.is_none()
    {
        state.storage.forget_routing_affinity(key, id).await?;
    }
    if request.previous_response.is_some()
        && !request.previous_can_move
        && previous_candidate.is_none()
    {
        return Err(context_required());
    }
    let weighted = settings.scheduler_mode == "advanced" && settings.sticky_weighted;
    let mut preserve = false;
    let hard = previous
        .as_ref()
        .filter(|_| !weighted || !request.previous_can_move)
        .or_else(|| sticky.as_ref().filter(|_| !weighted));
    if let Some(id) = hard
        && let Some(candidate) = previous_candidate
            .as_ref()
            .filter(|c| &c.id == id)
            .or_else(|| sticky_candidate.as_ref().filter(|c| &c.id == id))
    {
        let cannot_move = request.previous_response.is_some() && !request.previous_can_move;
        let metrics = state.storage.supplier_metrics(id);
        let unhealthy = metrics.error_rate >= 0.5 || metrics.ttft_ms.is_some_and(|v| v >= 15000.0);
        if !settings.sticky_escape || cannot_move || !unhealthy {
            if let Some(permit) = state
                .storage
                .acquire_supplier(id, candidate.concurrency)
                .await?
            {
                if let Some(key) = &sticky_key {
                    if sticky.as_ref() == Some(id) {
                        state
                            .storage
                            .refresh_routing_affinity(key, id, settings.sticky_ttl_seconds)
                            .await?;
                    } else {
                        state
                            .storage
                            .save_routing_affinity(key, id, settings.sticky_ttl_seconds)
                            .await?;
                    }
                }
                return Ok(SupplierContext {
                    account: state.storage.supplier_account_snapshot(id).await?,
                    permit: Some(permit),
                    reused_affinity: true,
                    previous_response_matches: previous.as_ref() == Some(id),
                    request: request.clone(),
                });
            }
        }
        if !settings.sticky_escape || cannot_move {
            let permit = state
                .storage
                .wait_supplier(
                    id,
                    candidate.concurrency,
                    std::time::Duration::from_millis(settings.pool_wait_ms),
                )
                .await?
                .ok_or_else(busy)?;
            state
                .storage
                .virtual_access(&oauth.token_hash)
                .await?
                .ok_or_else(ApiError::invalid_token)?;
            let current_route = state
                .storage
                .execution_route(&oauth.virtual_account_id, &oauth.provider_id)
                .await?;
            if !current_route.is_some_and(|current| {
                current.tag_id.as_deref() == Some(tag.as_str())
                    && current.revision == route.revision
            }) {
                return Err(crate::pool_execution::exhausted());
            }
            let current = state
                .storage
                .pool_candidate(&tag, &oauth.provider_id, request.model.as_deref(), id)
                .await?;
            let Some(current) = current else {
                drop(permit);
                return Err(if cannot_move {
                    context_required()
                } else {
                    crate::pool_execution::exhausted()
                });
            };
            if current.concurrency > 0
                && state.storage.supplier_metrics(id).active > current.concurrency
            {
                drop(permit);
                return Err(busy());
            }
            if let Some(key) = &sticky_key {
                if sticky.as_ref() == Some(id) {
                    state
                        .storage
                        .refresh_routing_affinity(key, id, settings.sticky_ttl_seconds)
                        .await?;
                } else {
                    state
                        .storage
                        .save_routing_affinity(key, id, settings.sticky_ttl_seconds)
                        .await?;
                }
            }
            return Ok(SupplierContext {
                account: state.storage.supplier_account_snapshot(id).await?,
                permit: Some(permit),
                reused_affinity: true,
                previous_response_matches: previous.as_ref() == Some(id),
                request: request.clone(),
            });
        }
        preserve = true;
        // Escaping relaxes affinity; health metrics are not an availability ban.
        // Keeping the owner eligible also lets a single-supplier pool recover.
    }
    let pool = state
        .storage
        .pool_candidates(&tag, &oauth.provider_id, request.model.as_deref())
        .await?;
    if pool.is_empty() {
        let model_unavailable = request.model.is_some()
            && !state
                .storage
                .pool_candidates(&tag, &oauth.provider_id, None)
                .await?
                .is_empty();
        return Err(if model_unavailable {
            ApiError::openai(
                http::StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
                "No eligible supplier in this pool has the requested model configured.",
                Some("supplier_model_unavailable"),
            )
        } else {
            crate::pool_execution::exhausted()
        });
    }
    let seed = serde_json::to_vec(&(
        &request.session_hash,
        &request.previous_response,
        &request.model,
        &tag,
        &oauth.virtual_account_id,
    ))
    .expect("schedule seed");
    let digest = Sha256::digest(&seed);
    let mut rng = Rng(u64::from_le_bytes(digest[..8].try_into().unwrap()).max(1));
    if request.session_hash.is_none() && request.previous_response.is_none() {
        rng.0 = (rng.0 ^ rand::random::<u64>()).max(1);
    }
    let mut attempted: std::collections::HashSet<String> = excluded.iter().cloned().collect();
    if !excluded.is_empty() && pool.iter().all(|account| attempted.contains(&account.id)) {
        return Err(crate::pool_execution::exhausted());
    }
    while let Some((candidate, mut permit)) = state
        .storage
        .acquire_ready_supplier(
            pool.clone(),
            settings.scheduler_mode == "traditional",
            settings.pool_top_k.max(1),
            previous
                .as_deref()
                .filter(|_| weighted && request.previous_can_move),
            sticky.as_deref().filter(|_| weighted),
            &attempted,
            rng.unit(),
            request.model.is_some(),
        )
        .await?
    {
        let id = &candidate.id;
        attempted.insert(id.clone());
        if request.model.is_some() && permit.is_none() {
            continue;
        }
        let Some(current) = state
            .storage
            .pool_candidate(&tag, &oauth.provider_id, request.model.as_deref(), id)
            .await?
        else {
            continue;
        };
        if request.model.is_some() && current.concurrency != candidate.concurrency {
            drop(permit.take());
            permit = state
                .storage
                .acquire_supplier(id, current.concurrency)
                .await?;
            if permit.is_none() {
                continue;
            }
        }
        if (!preserve || sticky.as_ref() == Some(id))
            && let Some(key) = &sticky_key
        {
            if sticky.as_ref() == Some(id) {
                state
                    .storage
                    .refresh_routing_affinity(key, id, settings.sticky_ttl_seconds)
                    .await?;
            } else {
                state
                    .storage
                    .save_routing_affinity(key, id, settings.sticky_ttl_seconds)
                    .await?;
            }
        }
        let reused_affinity = Some(id) == sticky.as_ref() || Some(id) == previous.as_ref();
        let previous_response_matches = previous.as_ref() == Some(id);
        let id = id.clone();
        return Ok(SupplierContext {
            account: state.storage.supplier_account_snapshot(&id).await?,
            permit,
            reused_affinity,
            previous_response_matches,
            request: request.clone(),
        });
    }
    Err(busy())
}

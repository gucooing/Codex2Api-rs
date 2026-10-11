//! Provider-neutral pool scheduling. Providers construct their own affinity seeds.
use crate::{ApiError, ApiState, Result, pool_execution::SupplierContext};
use codex2api_storage::{PoolCandidate, RuntimeMetrics, VirtualAccess};
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
    Some(
        serde_json::json!([
            body.get("model"),
            body.get("instructions").or_else(|| body.get("system")),
            system_messages,
            body.get("tools"),
            first
        ])
        .to_string(),
    )
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
#[derive(Clone)]
struct Ranked {
    account: PoolCandidate,
    metrics: RuntimeMetrics,
    load: u64,
    score: f64,
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
    let pool = state
        .storage
        .pool_candidates(&tag, &oauth.provider_id, request.model.as_deref())
        .await?;
    let model_candidates_exist = !pool.is_empty();
    let mut candidates: Vec<_> = pool
        .into_iter()
        .filter(|a| !excluded.contains(&a.id))
        .map(|account| {
            let metrics = state.storage.supplier_metrics(&account.id);
            let capacity = account
                .load_factor
                .filter(|v| *v > 0)
                .unwrap_or(account.concurrency.max(1));
            let load = 100 * (u64::from(metrics.active) + u64::from(metrics.waiting))
                / u64::from(capacity);
            Ranked {
                account,
                metrics,
                load,
                score: 0.0,
            }
        })
        .collect();
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
    if let (Some(key), Some(id)) = (&sticky_key, &sticky)
        && !candidates.iter().any(|v| &v.account.id == id)
    {
        state.storage.forget_routing_affinity(key, id).await?;
    }
    if request.previous_response.is_some()
        && !request.previous_can_move
        && (previous.is_none()
            || !candidates
                .iter()
                .any(|c| Some(&c.account.id) == previous.as_ref()))
    {
        return Err(context_required());
    }
    if candidates.is_empty() {
        let model_unavailable = request.model.is_some()
            && !model_candidates_exist
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
    let weighted = settings.scheduler_mode == "advanced" && settings.sticky_weighted;
    let mut preserve = false;
    let hard = previous
        .as_ref()
        .filter(|_| !weighted || !request.previous_can_move)
        .or_else(|| sticky.as_ref().filter(|_| !weighted));
    if let Some(id) = hard
        && let Some(candidate) = candidates.iter().find(|v| &v.account.id == id)
    {
        let cannot_move = request.previous_response.is_some() && !request.previous_can_move;
        let unhealthy = candidate.metrics.error_rate >= 0.5
            || candidate.metrics.ttft_ms.is_some_and(|v| v >= 15000.0);
        if !settings.sticky_escape || cannot_move || !unhealthy {
            if let Some(permit) = state
                .storage
                .acquire_supplier(id, candidate.account.concurrency)
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
                    account: state.storage.require_account(id).await?,
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
                    candidate.account.concurrency,
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
            let eligible = state
                .storage
                .pool_candidates(&tag, &oauth.provider_id, request.model.as_deref())
                .await?;
            let Some(current) = eligible.iter().find(|candidate| &candidate.id == id) else {
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
                account: state.storage.require_account(id).await?,
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
    if candidates.is_empty() {
        return Err(busy());
    }
    let seed = serde_json::json!([
        request.session_hash,
        request.previous_response,
        request.model,
        tag,
        oauth.virtual_account_id
    ]);
    let digest = Sha256::digest(seed.to_string().as_bytes());
    let mut rng = Rng(u64::from_le_bytes(digest[..8].try_into().unwrap()).max(1));
    if request.session_hash.is_none() && request.previous_response.is_none() {
        rng.0 ^= rand::random::<u64>();
        rng.0 = rng.0.max(1);
    }
    if settings.scheduler_mode == "traditional" {
        // Randomize ties before a stable priority/load/LRU sort.
        // Load ranks suppliers; atomic permit acquisition enforces concurrency.
        for i in (1..candidates.len()).rev() {
            let j = (rng.next() as usize) % (i + 1);
            candidates.swap(i, j);
        }
        candidates.sort_by_key(|v| (v.account.priority, v.load, v.metrics.last_used));
    } else {
        let min_p = candidates.iter().map(|v| v.account.priority).min().unwrap();
        let max_p = candidates.iter().map(|v| v.account.priority).max().unwrap();
        let max_q = candidates
            .iter()
            .map(|v| v.metrics.waiting)
            .max()
            .unwrap()
            .max(1) as f64;
        let min_t = candidates
            .iter()
            .filter_map(|v| v.metrics.ttft_ms)
            .fold(f64::INFINITY, f64::min);
        let max_t = candidates
            .iter()
            .filter_map(|v| v.metrics.ttft_ms)
            .fold(0.0, f64::max);
        for c in &mut candidates {
            let p = if min_p == max_p {
                1.0
            } else {
                1.0 - f64::from(c.account.priority - min_p) / f64::from(max_p - min_p)
            };
            let l = 1.0 - (c.load as f64 / 100.0).clamp(0.0, 1.0);
            let q = 1.0 - f64::from(c.metrics.waiting) / max_q;
            let e = 1.0 - c.metrics.error_rate.clamp(0.0, 1.0);
            let t = c
                .metrics
                .ttft_ms
                .filter(|_| max_t > min_t)
                .map_or(0.5, |t| {
                    1.0 - ((t - min_t) / (max_t - min_t)).clamp(0.0, 1.0)
                });
            c.score = p + l + 0.7 * q + 0.8 * e + 0.5 * t;
            if weighted {
                if Some(&c.account.id) == sticky.as_ref() {
                    c.score += 3.0
                }
                if request.previous_can_move && Some(&c.account.id) == previous.as_ref() {
                    c.score += 5.0
                }
            }
        }
        candidates.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then(a.account.priority.cmp(&b.account.priority))
                .then(a.load.cmp(&b.load))
                .then(a.metrics.waiting.cmp(&b.metrics.waiting))
                .then(a.account.id.cmp(&b.account.id))
        });
        candidates.truncate(settings.pool_top_k.max(1));
        let sticky_index = weighted
            .then(|| {
                previous
                    .as_ref()
                    .and_then(|id| candidates.iter().position(|c| &c.account.id == id))
                    .or_else(|| {
                        sticky
                            .as_ref()
                            .and_then(|id| candidates.iter().position(|c| &c.account.id == id))
                    })
            })
            .flatten();
        if let Some(i) = sticky_index {
            let first = candidates.remove(i);
            candidates.insert(0, first);
        } else {
            let minimum = candidates.last().unwrap().score;
            let mut ordered = Vec::with_capacity(candidates.len());
            while !candidates.is_empty() {
                let sum: f64 = candidates.iter().map(|v| v.score - minimum + 1.0).sum();
                let mut target = rng.unit() * sum;
                let mut index = candidates.len() - 1;
                for (i, c) in candidates.iter().enumerate() {
                    target -= c.score - minimum + 1.0;
                    if target < 0.0 {
                        index = i;
                        break;
                    }
                }
                ordered.push(candidates.remove(index));
            }
            candidates = ordered;
        }
    }
    for candidate in &candidates {
        let id = &candidate.account.id;
        let permit = if request.model.is_some() {
            state
                .storage
                .acquire_supplier(id, candidate.account.concurrency)
                .await?
        } else {
            None
        };
        if request.model.is_some() && permit.is_none() {
            continue;
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
        return Ok(SupplierContext {
            account: state.storage.require_account(id).await?,
            permit,
            reused_affinity: Some(id) == sticky.as_ref() || Some(id) == previous.as_ref(),
            previous_response_matches: previous.as_ref() == Some(id),
            request: request.clone(),
        });
    }
    Err(busy())
}

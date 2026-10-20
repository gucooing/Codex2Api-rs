//! The Antigravity channel's judgement of Cloud Code Assist failures. Quota
//! exhaustion blocks the routed model and its pool until Google's reset; capacity
//! shortages block the model briefly; throttling retries the same account with
//! Google's retry delay before switching; the desktop client's known intermittent
//! errors retry in place.
use super::errors;
use crate::supply::{AvoidScope, AvoidTarget, TransportFault, UpstreamFailure, Verdict};
use codex2api_storage::BlockKind;
use std::time::Duration;

pub(crate) fn verdict(failure: &UpstreamFailure) -> Verdict {
    let now = failure.now;
    if let Some(fault) = failure.transport {
        return match fault {
            TransportFault::Connect => Verdict::avoid(
                AvoidScope::Until(now.saturating_add(600)),
                AvoidTarget::Account,
                BlockKind::Transport,
                Some("upstream_connection_failed"),
                None,
            ),
            TransportFault::Timeout => Verdict::avoid_request(),
            TransportFault::Stream => Verdict::retry_same(
                Duration::from_millis(500),
                1,
                Verdict::avoid(
                    AvoidScope::Until(now.saturating_add(300)),
                    AvoidTarget::Account,
                    BlockKind::Penalty,
                    Some("empty_response"),
                    None,
                ),
            ),
        };
    }
    let parsed = errors::parsed(&failure.body);
    let status = failure.status.or(parsed.status);
    let reason = parsed.reason.unwrap_or_default().to_ascii_lowercase();
    let rpc = parsed.rpc_status.unwrap_or_default().to_ascii_uppercase();
    let message = parsed.message.map(str::to_owned);
    let lower = message.clone().unwrap_or_default().to_ascii_lowercase();
    let code = parsed
        .reason
        .or(parsed.rpc_status)
        .map(|v| v.to_ascii_lowercase());
    let model = parsed
        .model
        .clone()
        .or_else(|| failure.model.clone())
        .map_or(AvoidTarget::Account, AvoidTarget::Model);
    let delay = parsed
        .retry_after
        .map(|seconds| seconds.ceil().max(1.0) as i64);
    if failure.output_committed {
        return Verdict::Stop {
            status: status.unwrap_or(502),
            body: failure.body.clone(),
        };
    }
    let exhausted = matches!(reason.as_str(), "quota_exhausted" | "quota_exceeded")
        || lower.contains("exhausted your capacity")
        || lower.contains("quota exceeded");
    match status {
        Some(401) => Verdict::avoid(
            AvoidScope::Permanent,
            AvoidTarget::Account,
            BlockKind::Auth,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(403) => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(600)),
            AvoidTarget::Account,
            BlockKind::Penalty,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(404) => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(3600)),
            model,
            BlockKind::ModelMissing,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(429) | Some(_) if exhausted => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(delay.unwrap_or(60))),
            model,
            BlockKind::Quota,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(429) => Verdict::retry_same(
            Duration::from_secs(delay.unwrap_or(1).clamp(1, 8) as u64),
            3,
            Verdict::avoid(
                AvoidScope::Until(now.saturating_add(delay.unwrap_or(60))),
                AvoidTarget::Account,
                BlockKind::RateLimit,
                code.as_deref(),
                message.as_deref(),
            ),
        ),
        Some(400) if lower.contains("invalid project resource name") => Verdict::retry_same(
            Duration::from_secs(1),
            2,
            Verdict::avoid(
                AvoidScope::Until(now.saturating_add(600)),
                AvoidTarget::Account,
                BlockKind::Penalty,
                code.as_deref(),
                message.as_deref(),
            ),
        ),
        Some(status @ 400..=499) => Verdict::ClientError {
            status,
            body: failure.body.clone(),
        },
        Some(503) if reason == "model_capacity_exhausted" => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(delay.unwrap_or(60))),
            model,
            BlockKind::Overload,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(500) if rpc == "INTERNAL" => Verdict::retry_same(
            Duration::from_secs(1),
            2,
            Verdict::avoid(
                AvoidScope::Until(now.saturating_add(1800)),
                AvoidTarget::Account,
                BlockKind::Penalty,
                code.as_deref(),
                message.as_deref(),
            ),
        ),
        Some(529) => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(600)),
            AvoidTarget::Account,
            BlockKind::Overload,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(_) => Verdict::retry_same(Duration::from_secs(1), 2, Verdict::avoid_request()),
        None if rpc == "RESOURCE_EXHAUSTED" => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(delay.unwrap_or(60))),
            AvoidTarget::Account,
            BlockKind::RateLimit,
            code.as_deref(),
            message.as_deref(),
        ),
        None if matches!(rpc.as_str(), "INVALID_ARGUMENT" | "FAILED_PRECONDITION") => {
            Verdict::ClientError {
                status: 400,
                body: failure.body.clone(),
            }
        }
        None => Verdict::avoid_request(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;
    use serde_json::{Value, json};

    fn failure(status: Option<u16>, body: Value) -> UpstreamFailure {
        UpstreamFailure {
            status,
            headers: HeaderMap::new(),
            body,
            transport: None,
            model: Some("claude-opus-5.5".into()),
            output_committed: false,
            now: 100,
        }
    }

    #[test]
    fn quota_exhaustion_blocks_the_model_and_throttling_retries_in_place() {
        let exhausted = json!({"error":{"code":429,"message":"You have exhausted your capacity on this model.","status":"RESOURCE_EXHAUSTED",
            "details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"4m50s"},
                       {"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"QUOTA_EXHAUSTED","metadata":{"model":"publishers/anthropic/models/claude-opus-5-5"}}]}});
        match verdict(&failure(Some(429), exhausted)) {
            Verdict::Avoid {
                scope,
                target: AvoidTarget::Model(model),
                kind: BlockKind::Quota,
                ..
            } => {
                assert_eq!(scope, AvoidScope::Until(390));
                assert_eq!(model, "claude-opus-5-5");
            }
            other => panic!("{other:?}"),
        }
        let throttled = json!({"error":{"code":429,"message":"Resource has been exhausted (e.g. check quota).","status":"RESOURCE_EXHAUSTED",
            "details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"RATE_LIMIT_EXCEEDED","metadata":{"model":"gemini-3-pro-high"}}]}});
        assert!(matches!(
            verdict(&failure(Some(429), throttled)),
            Verdict::RetrySame { max: 3, .. }
        ));
        let capacity = json!({"error":{"code":503,"message":"The model is overloaded.","status":"UNAVAILABLE",
            "details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"MODEL_CAPACITY_EXHAUSTED","metadata":{"model":"gemini-3-pro-high"}}]}});
        assert!(matches!(
            verdict(&failure(Some(503), capacity)),
            Verdict::Avoid {
                target: AvoidTarget::Model(_),
                kind: BlockKind::Overload,
                ..
            }
        ));
        assert!(matches!(
            verdict(&failure(
                Some(400),
                json!({"error":{"code":400,"message":"bad schema","status":"INVALID_ARGUMENT"}})
            )),
            Verdict::ClientError { status: 400, .. }
        ));
    }
}

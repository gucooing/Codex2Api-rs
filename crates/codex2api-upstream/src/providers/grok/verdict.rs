//! The Grok channel's judgement of its own upstream failures, over Grok Build's
//! normalized error envelopes. Content-policy refusals belong to the request; account
//! access rejections are permanent; free-usage exhaustion blocks until its reset.
use super::errors;
use crate::supply::{
    AvoidScope, AvoidTarget, TransportFault, UpstreamFailure, Verdict, retry_after,
};
use codex2api_storage::BlockKind;
use http::HeaderMap;
use serde_json::Value;
use std::time::Duration;

/// The same provider evidence drives the block kind and the ledger category.
pub(crate) fn quota_code(status: Option<u16>, error: &Value) -> Option<&str> {
    if status.is_some_and(|status| !matches!(status, 402 | 403 | 429)) {
        return None;
    }
    ["code", "type"]
        .into_iter()
        .filter_map(|key| error[key].as_str().map(str::trim))
        .find(|code| {
            matches!(
                *code,
                errors::FREE_USAGE_EXHAUSTED
                    | "personal-team-blocked:spending-limit"
                    | "insufficient_quota"
                    | "insufficient_credits"
                    | "billing_error"
            )
        })
        .or_else(|| {
            // The observed IC free-tier rejection can lack a WKE/semantic code.
            // Match its affirmative exhaustion sentence, never generic quota/upgrade wording.
            let message = error["message"].as_str()?.trim();
            let bare_or_throttle = error["code"].is_null()
                || matches!(
                    error["code"].as_str(),
                    Some("429" | "rate_limit_exceeded" | "resource-exhausted")
                )
                || error["code"].as_u64() == Some(429);
            let quota_or_throttle_type = error["type"].is_null()
                || matches!(
                    error["type"].as_str(),
                    Some(
                        "error"
                            | "rate_limit_error"
                            | "rate_limit_exceeded"
                            | "resource-exhausted"
                            | "quota_exhausted"
                    )
                );
            (status == Some(429)
                && bare_or_throttle
                && quota_or_throttle_type
                && [
                    "You've used all the included free usage for model ",
                    "You’ve used all the included free usage for model ",
                ]
                .iter()
                .any(|prefix| message.starts_with(prefix))
                && message.contains(" for now.")
                && message.contains("Usage resets over a rolling "))
            .then_some(errors::FREE_USAGE_EXHAUSTED)
        })
}

/// When a throttled or exhausted account may be used again: structured retry fields,
/// the `Retry-After` header, a "rolling N hour" sentence, or a short probe.
fn retry_at(error: &Value, headers: &HeaderMap, now: i64) -> i64 {
    let seconds = ["retry_after_seconds", "retry_after", "retryAfter"]
        .into_iter()
        .find_map(|key| error[key].as_f64())
        .or_else(|| error["retry_after_ms"].as_f64().map(|ms| ms / 1000.0))
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map(|n| now.saturating_add(n.ceil() as i64));
    let absolute = ["resets_at", "reset_at", "retry_at"]
        .into_iter()
        .find_map(|key| error[key].as_i64())
        .filter(|t| *t > now);
    let rolling = error["message"].as_str().and_then(|message| {
        let lower = message.to_ascii_lowercase();
        let start = lower.find("rolling ")? + "rolling ".len();
        let rest = &lower[start..];
        let digits = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let count: i64 = rest[..digits].parse().ok()?;
        let unit = rest[digits..].trim_start();
        let seconds = if unit.starts_with("hour") {
            3600
        } else if unit.starts_with("minute") {
            60
        } else if unit.starts_with("day") {
            86400
        } else {
            return None;
        };
        Some(now.saturating_add(count.saturating_mul(seconds)))
    });
    [seconds, absolute, retry_after(headers, now), rolling]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(now.saturating_add(60))
        .max(now.saturating_add(1))
}

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
            TransportFault::Stream => {
                Verdict::retry_same(Duration::from_millis(500), 1, Verdict::avoid_request())
            }
        };
    }
    let error = errors::normalize(&failure.body);
    let status = failure.status.or_else(|| errors::status(&error));
    let message = error["message"].as_str().map(str::to_owned);
    let code = error["code"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| error["type"].as_str().map(str::to_owned));
    if failure.output_committed {
        return Verdict::Stop {
            status: status.unwrap_or(502),
            body: failure.body.clone(),
        };
    }
    if status == Some(401) {
        return Verdict::avoid(
            AvoidScope::Permanent,
            AvoidTarget::Account,
            BlockKind::Auth,
            code.as_deref(),
            message.as_deref(),
        );
    }
    if let Some(quota) = quota_code(status, &error) {
        return Verdict::avoid(
            AvoidScope::Until(retry_at(&error, &failure.headers, now)),
            AvoidTarget::Account,
            BlockKind::Quota,
            Some(quota),
            message.as_deref(),
        );
    }
    let lower = message.clone().unwrap_or_default().to_ascii_lowercase();
    match status {
        Some(402) => Verdict::avoid(
            AvoidScope::Permanent,
            AvoidTarget::Account,
            BlockKind::Billing,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(403)
            if lower.contains("content policy")
                || lower.contains("safety")
                || lower.contains("moderation") =>
        {
            Verdict::ClientError {
                status: 403,
                body: failure.body.clone(),
            }
        }
        Some(403)
            if lower.contains("suspend")
                || lower.contains("banned")
                || lower.contains("disabled")
                || lower.contains("terminated") =>
        {
            Verdict::avoid(
                AvoidScope::Permanent,
                AvoidTarget::Account,
                BlockKind::Auth,
                code.as_deref(),
                message.as_deref(),
            )
        }
        Some(403) => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(600)),
            AvoidTarget::Account,
            BlockKind::Penalty,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(404) if lower.contains("model") => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(3600)),
            failure
                .model
                .clone()
                .map_or(AvoidTarget::Account, AvoidTarget::Model),
            BlockKind::ModelMissing,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(429) => Verdict::avoid(
            AvoidScope::Until(retry_at(&error, &failure.headers, now)),
            AvoidTarget::Account,
            BlockKind::RateLimit,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(529) => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(600)),
            AvoidTarget::Account,
            BlockKind::Overload,
            code.as_deref(),
            message.as_deref(),
        ),
        Some(status @ 400..=499) => Verdict::ClientError {
            status,
            body: failure.body.clone(),
        },
        Some(_) => Verdict::avoid_request(),
        None if matches!(
            code.as_deref(),
            Some("rate_limit_exceeded" | "slow_down" | "rate_limit_error")
        ) =>
        {
            Verdict::avoid(
                AvoidScope::Until(retry_at(&error, &failure.headers, now)),
                AvoidTarget::Account,
                BlockKind::RateLimit,
                code.as_deref(),
                message.as_deref(),
            )
        }
        None if matches!(
            code.as_deref(),
            Some("invalid_request_error" | "invalid_prompt" | "context_length_exceeded")
        ) =>
        {
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
    use serde_json::json;

    fn failure(status: Option<u16>, body: Value) -> UpstreamFailure {
        UpstreamFailure {
            status,
            headers: HeaderMap::new(),
            body,
            transport: None,
            model: Some("grok-4".into()),
            output_committed: false,
            now: 100,
        }
    }

    #[test]
    fn free_usage_exhaustion_blocks_until_its_rolling_reset() {
        let body = json!({"error":{"code":"429","message":"You've used all the included free usage for model grok-4 for now. Usage resets over a rolling 24 hour window."}});
        match verdict(&failure(Some(429), body)) {
            Verdict::Avoid {
                scope, kind, code, ..
            } => {
                assert_eq!(scope, AvoidScope::Until(100 + 86400));
                assert_eq!(kind, BlockKind::Quota);
                assert_eq!(code.as_deref(), Some(errors::FREE_USAGE_EXHAUSTED));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            verdict(&failure(
                Some(429),
                json!({"error":{"code":"rate_limit_exceeded","message":"slow"}})
            )),
            Verdict::Avoid {
                kind: BlockKind::RateLimit,
                ..
            }
        ));
        assert!(matches!(
            verdict(&failure(
                Some(403),
                json!({"error":{"message":"Request blocked by content policy"}})
            )),
            Verdict::ClientError { status: 403, .. }
        ));
    }
}

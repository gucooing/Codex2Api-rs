//! The ChatGPT channel's judgement of its own upstream failures. Credential
//! rejection after the inline refresh and HTTP 402 are permanent; quota windows block
//! until their official reset; throttling blocks until the retry advice; model
//! entitlement failures block one model; everything server-side switches suppliers.
use crate::supply::{
    AvoidScope, AvoidTarget, TransportFault, UpstreamFailure, Verdict, retry_after,
};
use codex2api_storage::BlockKind;
use http::HeaderMap;
use serde_json::Value;
use std::time::Duration;

const QUOTA_CODES: [&str; 7] = [
    "usage_limit_reached",
    "usage_limit_exceeded",
    "insufficient_quota",
    "credit_balance_exhausted",
    "organization_spend_limit_exceeded",
    "project_spend_limit_exceeded",
    "organization_usage_limit_exceeded",
];

/// Recovery requires affirmative official evidence, not simply the absence of a
/// recognized exhaustion. Partial responses must leave persisted health alone.
pub fn quota_available(value: &Value) -> bool {
    value.get("rate_limit").is_some_and(Value::is_object)
        && (value
            .pointer("/rate_limit/allowed")
            .and_then(Value::as_bool)
            == Some(true)
            || value.pointer("/credits/unlimited").and_then(Value::as_bool) == Some(true)
            || value
                .pointer("/credits/has_credits")
                .and_then(Value::as_bool)
                == Some(true))
}

/// Only the main official execution allowance can block the whole account.
/// Additional model-specific limits and a full window backed by credits do not.
pub fn quota_unavailable_until(value: &Value, now: i64) -> Option<i64> {
    let rate = value.get("rate_limit")?;
    if rate.get("allowed").and_then(Value::as_bool) != Some(false) {
        return None;
    }
    if value.pointer("/credits/unlimited").and_then(Value::as_bool) == Some(true)
        || value
            .pointer("/credits/has_credits")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return None;
    }
    let reset = ["primary_window", "secondary_window"]
        .into_iter()
        .filter_map(|key| {
            let window = rate.get(key)?;
            if window.get("used_percent").and_then(Value::as_f64)? < 100.0 {
                return None;
            }
            window.get("reset_at").and_then(Value::as_i64).or_else(|| {
                window
                    .get("reset_after_seconds")
                    .and_then(Value::as_i64)
                    .map(|n| now.saturating_add(n))
            })
        })
        .filter(|n| *n > now)
        .max();
    Some(reset.unwrap_or(now.saturating_add(60)))
}

/// The reset of an exhausted window from the error body and the official quota headers.
fn quota_reset(error: &Value, headers: &HeaderMap, now: i64) -> i64 {
    let reset = error
        .get("resets_at")
        .and_then(Value::as_i64)
        .filter(|t| *t > now);
    let family = headers
        .get("x-codex-active-limit")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("codex")
        .replace('_', "-");
    let window_reset = ["primary", "secondary"]
        .into_iter()
        .filter_map(|window| {
            let used = headers
                .get(format!("x-{family}-{window}-used-percent"))?
                .to_str()
                .ok()?
                .parse::<f64>()
                .ok()?;
            if used < 100.0 {
                return None;
            }
            headers
                .get(format!("x-{family}-{window}-reset-at"))
                .and_then(|h| h.to_str().ok())?
                .parse::<i64>()
                .ok()
        })
        .filter(|t| *t > now)
        .max();
    // Absent official quota timing means a short probe, never a fabricated reset.
    [reset, retry_after(headers, now), window_reset]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(now.saturating_add(60))
        .max(now.saturating_add(1))
}

/// Headers an error body carries when the failure arrived inside a stream.
fn observed_headers(failure: &UpstreamFailure) -> HeaderMap {
    let mut headers = failure.headers.clone();
    if let Some(fields) = failure.body.get("headers").and_then(Value::as_object) {
        for (name, value) in fields {
            if let Some(text) = value
                .as_str()
                .or_else(|| value.as_array()?.first()?.as_str())
                && let Ok(name) = http::header::HeaderName::from_bytes(name.as_bytes())
                && let Ok(value) = http::HeaderValue::from_str(text)
            {
                headers.insert(name, value);
            }
        }
    }
    headers
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
    let parsed = crate::ResponseFailure::from_error(failure.status, &failure.body);
    let status = parsed.status;
    let code = parsed.code.clone().unwrap_or_default();
    let error = crate::supply::error_object(&failure.body);
    let kind = error
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let quota = QUOTA_CODES.contains(&code.as_str()) || QUOTA_CODES.contains(&kind);
    if failure.output_committed {
        return Verdict::Stop {
            status: status.unwrap_or(502),
            body: failure.body.clone(),
        };
    }
    let model_entitlement = matches!(
        code.as_str(),
        "model_not_found" | "model_not_allowed" | "usage_not_included"
    ) || kind == "usage_not_included";
    let model_target = || {
        failure
            .model
            .clone()
            .map_or(AvoidTarget::Account, AvoidTarget::Model)
    };
    match status {
        Some(401) => Verdict::avoid(
            AvoidScope::Permanent,
            AvoidTarget::Account,
            BlockKind::Auth,
            Some(&code),
            parsed.message.as_deref(),
        ),
        Some(402) => Verdict::avoid(
            AvoidScope::Permanent,
            AvoidTarget::Account,
            BlockKind::Billing,
            parsed.code.as_deref(),
            parsed.message.as_deref(),
        ),
        Some(403) if model_entitlement => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(3600)),
            model_target(),
            BlockKind::ModelMissing,
            Some(&code),
            parsed.message.as_deref(),
        ),
        Some(403) => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(600)),
            AvoidTarget::Account,
            BlockKind::Penalty,
            Some(&code),
            parsed.message.as_deref(),
        ),
        Some(404) if model_entitlement => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(3600)),
            model_target(),
            BlockKind::ModelMissing,
            Some(&code),
            parsed.message.as_deref(),
        ),
        Some(429) if quota => Verdict::avoid(
            AvoidScope::Until(quota_reset(error, &observed_headers(failure), now)),
            AvoidTarget::Account,
            BlockKind::Quota,
            Some(&code),
            parsed.message.as_deref(),
        ),
        Some(429) => Verdict::avoid(
            AvoidScope::Until(
                retry_after(&observed_headers(failure), now)
                    .unwrap_or(now.saturating_add(60))
                    .max(now.saturating_add(1)),
            ),
            AvoidTarget::Account,
            BlockKind::RateLimit,
            Some(&code),
            parsed.message.as_deref(),
        ),
        Some(529) => Verdict::avoid(
            AvoidScope::Until(now.saturating_add(600)),
            AvoidTarget::Account,
            BlockKind::Overload,
            Some(&code),
            parsed.message.as_deref(),
        ),
        Some(status @ 400..=499) => Verdict::ClientError {
            status,
            body: failure.body.clone(),
        },
        Some(_) => Verdict::avoid_request(),
        // A failure event before any output without an HTTP status: quota and
        // throttling codes keep their meaning, caller errors return, the rest switch.
        None if quota => Verdict::avoid(
            AvoidScope::Until(quota_reset(error, &observed_headers(failure), now)),
            AvoidTarget::Account,
            BlockKind::Quota,
            Some(&code),
            parsed.message.as_deref(),
        ),
        None if matches!(code.as_str(), "rate_limit_exceeded" | "slow_down")
            || kind == "rate_limit_error" =>
        {
            Verdict::avoid(
                AvoidScope::Until(
                    retry_after(&observed_headers(failure), now).unwrap_or(now.saturating_add(60)),
                ),
                AvoidTarget::Account,
                BlockKind::RateLimit,
                Some(&code),
                parsed.message.as_deref(),
            )
        }
        None if parsed.kind == crate::FailureKind::InvalidRequest => Verdict::ClientError {
            status: 400,
            body: failure.body.clone(),
        },
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
            model: Some("gpt-5".into()),
            output_committed: false,
            now: 100,
        }
    }

    #[test]
    fn quota_windows_block_until_reset_and_throttling_until_retry_advice() {
        let quota = failure(
            Some(429),
            json!({"error":{"code":"usage_limit_reached","resets_at":2000}}),
        );
        match verdict(&quota) {
            Verdict::Avoid { scope, kind, .. } => {
                assert_eq!(scope, AvoidScope::Until(2000));
                assert_eq!(kind, BlockKind::Quota);
            }
            other => panic!("{other:?}"),
        }
        let mut throttled = failure(
            Some(429),
            json!({"error":{"code":"rate_limit_exceeded","message":"slow"}}),
        );
        throttled
            .headers
            .insert("retry-after", "7".parse().unwrap());
        match verdict(&throttled) {
            Verdict::Avoid { scope, kind, .. } => {
                assert_eq!(scope, AvoidScope::Until(107));
                assert_eq!(kind, BlockKind::RateLimit);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn caller_errors_return_and_credentials_and_billing_block_permanently() {
        assert!(matches!(
            verdict(&failure(
                Some(400),
                json!({"error":{"code":"invalid_prompt"}})
            )),
            Verdict::ClientError { status: 400, .. }
        ));
        for (status, kind) in [(401, BlockKind::Auth), (402, BlockKind::Billing)] {
            match verdict(&failure(Some(status), Value::Null)) {
                Verdict::Avoid {
                    scope: AvoidScope::Permanent,
                    kind: observed,
                    ..
                } => assert_eq!(observed, kind),
                other => panic!("{other:?}"),
            }
        }
        match verdict(&failure(
            Some(403),
            json!({"error":{"type":"usage_not_included"}}),
        )) {
            Verdict::Avoid {
                target: AvoidTarget::Model(model),
                kind: BlockKind::ModelMissing,
                ..
            } => assert_eq!(model, "gpt-5"),
            other => panic!("{other:?}"),
        }
        let mut committed = failure(Some(503), Value::Null);
        committed.output_committed = true;
        assert!(matches!(verdict(&committed), Verdict::Stop { .. }));
        assert!(matches!(
            verdict(&failure(Some(503), Value::Null)),
            Verdict::Avoid {
                scope: AvoidScope::Request,
                ..
            }
        ));
    }
}

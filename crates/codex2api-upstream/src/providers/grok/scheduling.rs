//! Grok recovery policy. Shared pools only receive classified account unavailability.
use super::errors;
use crate::SupplierFailure;
use http::HeaderMap;
use serde_json::Value;

/// Grok Build refreshes only a rejected credential (401), not a 403 policy denial.
/// Only explicit quota exhaustion permits failover. Throttling, request errors,
/// capacity/edge failures and unknown errors retain the client's retry policy.
pub fn classify_failure(
    status: Option<u16>,
    value: &Value,
    headers: &HeaderMap,
    now: i64,
) -> Option<SupplierFailure> {
    if status == Some(401) {
        return Some(SupplierFailure::Authentication);
    }
    if status.is_none()
        && !value.get("error").is_some_and(|v| !v.is_null())
        && !value
            .pointer("/response/error")
            .is_some_and(|v| !v.is_null())
        && !matches!(value["type"].as_str(), Some("error" | "response.failed"))
    {
        return None;
    }
    let error = errors::normalize(value);
    let status = status.or_else(|| errors::status(&error));
    if status == Some(401) {
        return Some(SupplierFailure::Authentication);
    }
    if status.is_some_and(|status| !matches!(status, 402 | 403 | 429)) {
        return None;
    }
    let code = ["code", "type"]
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
                        "error" | "rate_limit_error" | "rate_limit_exceeded" | "resource-exhausted"
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
        })?;
    Some(SupplierFailure::QuotaExhausted {
        until: retry_at(&error, headers, now),
        code: code.into(),
        model: None,
    })
}

fn retry_at(error: &Value, headers: &HeaderMap, now: i64) -> i64 {
    let resets = ["reset_at", "resets_at", "retry_at"]
        .into_iter()
        .filter_map(|key| error[key].as_i64())
        .filter(|time| *time > now);
    let delays = ["retry_after", "retry_after_seconds", "retry_after_ms"]
        .into_iter()
        .filter_map(|key| {
            let seconds = error[key].as_f64()? / if key == "retry_after_ms" { 1000.0 } else { 1.0 };
            delay_until(seconds, now)
        });
    let http_retry = ["retry-after", "retry-after-ms"]
        .into_iter()
        .filter_map(|key| {
            let value = headers.get(key)?.to_str().ok()?;
            if key == "retry-after-ms" {
                delay_until(value.parse::<f64>().ok()? / 1000.0, now)
            } else {
                parse_retry_after(value, now)
            }
        });
    let event_retry = error["headers"].as_object().into_iter().flat_map(|fields| {
        fields.iter().filter_map(|(key, value)| {
            if key.eq_ignore_ascii_case("retry-after-ms") {
                let millis = value
                    .as_f64()
                    .or_else(|| value.as_str()?.parse::<f64>().ok())?;
                return delay_until(millis / 1000.0, now);
            }
            if !key.eq_ignore_ascii_case("retry-after") {
                return None;
            }
            value
                .as_str()
                .and_then(|value| parse_retry_after(value, now))
                .or_else(|| value.as_f64().and_then(|seconds| delay_until(seconds, now)))
        })
    });
    // A rolling 24-hour window is not an exact reset timestamp. With no explicit
    // timing, briefly exclude the account before another real execution can probe it.
    resets
        .chain(delays)
        .chain(http_retry)
        .chain(event_retry)
        .max()
        .unwrap_or_else(|| now.saturating_add(60))
        .max(now.saturating_add(1))
}

fn delay_until(seconds: f64, now: i64) -> Option<i64> {
    (seconds.is_finite() && seconds >= 0.0)
        .then(|| now.saturating_add(seconds.ceil().min(i64::MAX as f64) as i64))
}

fn parse_retry_after(value: &str, now: i64) -> Option<i64> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .and_then(|seconds| delay_until(seconds, now))
        .or_else(|| {
            chrono::DateTime::parse_from_rfc2822(value)
                .ok()
                .map(|time| time.timestamp())
                .filter(|time| *time > now)
        })
}

/// Prelude buffering is identical for the Grok OAuth and API listeners.
pub fn prelude(value: &Value) -> bool {
    if matches!(value["type"].as_str(), Some("message_start" | "ping")) {
        return !crate::scheduler::generation_observed(value);
    }
    if value["object"] == "chat.completion.chunk" {
        return value["choices"].as_array().is_some_and(|rows| {
            rows.iter().all(|row| {
                row["delta"]["content"].as_str().is_none_or(str::is_empty)
                    && row["delta"]["tool_calls"].is_null()
                    && row["delta"]["function_call"].is_null()
                    && row["delta"]["reasoning_content"]
                        .as_str()
                        .is_none_or(str::is_empty)
                    && row["finish_reason"].is_null()
            })
        });
    }
    crate::scheduler::prelude_event(value)
}

#[cfg(test)]
#[path = "scheduling_tests.rs"]
mod tests;

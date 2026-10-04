//! ChatGPT-specific supplier recovery semantics. Pool selection stays provider-neutral.
use crate::SupplierFailure;
use http::HeaderMap;
use serde_json::Value;

/// Only the main official execution allowance can cool the whole account.
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

/// Only credential rejection and explicit quota exhaustion remove pool supply.
/// Ordinary throttling, including an unclassified 429, is forwarded for the
/// official client to pause/retry; it never changes supplier health or binding.
pub fn classify_supplier_failure(
    status: Option<u16>,
    value: &Value,
    headers: &HeaderMap,
    now: i64,
) -> Option<SupplierFailure> {
    let error = value
        .pointer("/response/error")
        .or_else(|| value.get("error"))
        .unwrap_or(value);
    let status = status.or_else(|| {
        value
            .get("status")
            .and_then(Value::as_u64)
            .and_then(|n| u16::try_from(n).ok())
    });
    if status == Some(401) {
        return Some(SupplierFailure::Authentication);
    }
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .or_else(|| error.get("type").and_then(Value::as_str))
        .unwrap_or("");
    let code = match error.get("type").and_then(Value::as_str) {
        Some(kind @ ("usage_limit_reached" | "insufficient_quota" | "usage_not_included")) => kind,
        _ => code,
    };
    if !matches!(
        code,
        "usage_limit_reached"
            | "insufficient_quota"
            | "credit_balance_exhausted"
            | "organization_spend_limit_exceeded"
            | "project_spend_limit_exceeded"
            | "organization_usage_limit_exceeded"
    ) {
        return None;
    }
    let mut observed_headers = headers.clone();
    if let Some(fields) = value.get("headers").and_then(Value::as_object) {
        for (name, value) in fields {
            if let Some(text) = value
                .as_str()
                .or_else(|| value.as_array()?.first()?.as_str())
                && let Ok(name) = http::header::HeaderName::from_bytes(name.as_bytes())
                && let Ok(value) = http::HeaderValue::from_str(text)
            {
                observed_headers.insert(name, value);
            }
        }
    }
    let headers = &observed_headers;
    let reset = error
        .get("resets_at")
        .and_then(Value::as_i64)
        .filter(|t| *t > now);
    let retry = headers
        .get("retry-after")
        .and_then(|h| h.to_str().ok())
        .and_then(|v| {
            v.parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .map(|n| now.saturating_add(n.ceil().min(i64::MAX as f64) as i64))
                .or_else(|| {
                    chrono::DateTime::parse_from_rfc2822(v)
                        .ok()
                        .map(|t| t.timestamp())
                })
        });
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
    let until = [reset, retry, window_reset]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(now.saturating_add(60))
        .max(now.saturating_add(1));
    Some(SupplierFailure::QuotaExhausted {
        until,
        code: code.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_explicit_quota_exhaustion_or_authentication_change_availability() {
        let headers = HeaderMap::new();
        assert_eq!(
            classify_supplier_failure(
                Some(429),
                &json!({"error":{"type":"usage_limit_reached","resets_at":1000}}),
                &headers,
                100
            ),
            Some(SupplierFailure::QuotaExhausted {
                until: 1000,
                code: "usage_limit_reached".into()
            })
        );
        assert_eq!(
            classify_supplier_failure(Some(401), &Value::Null, &headers, 100),
            Some(SupplierFailure::Authentication)
        );
        for code in [
            "rate_limit_exceeded",
            "slow_down",
            "rate_limit_error",
            "http_429",
            "invalid_prompt",
            "usage_not_included",
            "flex_unavailable",
            "cyber_policy",
        ] {
            assert!(
                classify_supplier_failure(
                    Some(429),
                    &json!({"error":{"code":code}}),
                    &headers,
                    100
                )
                .is_none()
            );
            assert!(classify_supplier_failure(None,&json!({"type":"response.failed","response":{"error":{"code":code,"message":"Please try again in 11.054s."}}}),&headers,100).is_none());
        }
        for status in [403, 429, 500, 502, 504] {
            assert!(classify_supplier_failure(Some(status), &Value::Null, &headers, 100).is_none());
        }
        let refresh = crate::UpstreamError::Refresh(codex2api_auth::AuthError::RefreshRejected {
            status: 429,
            code: Some("slow_down".into()),
            message: "retry later".into(),
        });
        assert!(refresh.supplier_failure(100).is_none());
    }
}

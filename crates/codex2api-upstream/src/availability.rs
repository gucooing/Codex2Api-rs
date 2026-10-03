//! ChatGPT-specific supplier recovery semantics. Pool selection stays provider-neutral.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupplierFailure {
    Authentication,
    Cooldown {
        kind: &'static str,
        until: i64,
        code: String,
    },
}

impl crate::UpstreamError {
    pub fn supplier_failure(&self, now: i64) -> Option<SupplierFailure> {
        match self {
            Self::Unauthorized | Self::MissingAccessToken(_) => {
                Some(SupplierFailure::Authentication)
            }
            Self::Refresh(e) | Self::Auth(e) if e.permanent_refresh_failure() => {
                Some(SupplierFailure::Authentication)
            }
            Self::Refresh(codex2api_auth::AuthError::RefreshRejected {
                status: 429,
                code,
                message,
            })
            | Self::Auth(codex2api_auth::AuthError::RefreshRejected {
                status: 429,
                code,
                message,
            }) => classify_supplier_failure(
                Some(429),
                &serde_json::json!({"error":{"code":code,"message":message}}),
                &HeaderMap::new(),
                now,
            ),
            Self::Status {
                status,
                body,
                headers,
            } => classify_supplier_failure(
                Some(*status),
                &serde_json::from_str(body).unwrap_or_default(),
                headers,
                now,
            ),
            _ => None,
        }
    }
}

/// Official codex-api distinguishes subscription exhaustion from request/TPM
/// throttling and from usage_not_included, Flex capacity and policy rejections.
pub fn classify_supplier_failure(
    status: Option<u16>,
    value: &Value,
    headers: &HeaderMap,
    now: i64,
) -> Option<SupplierFailure> {
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
    let kind = match code {
        "usage_limit_reached"
        | "insufficient_quota"
        | "credit_balance_exhausted"
        | "organization_spend_limit_exceeded"
        | "project_spend_limit_exceeded"
        | "organization_usage_limit_exceeded" => "quota_exhausted",
        "rate_limit_exceeded" | "slow_down" | "rate_limit_error" => "rate_limited",
        "usage_not_included"
        | "flex_unavailable"
        | "flex_unavailable_error"
        | "invalid_prompt"
        | "cyber_policy"
        | "bio_policy" => return None,
        _ if status == Some(429) => "rate_limited",
        _ => return None,
    };
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
    // Only known throttling codes interpret the official plaintext retry delay.
    let message_retry = (kind == "rate_limited")
        .then(|| {
            error
                .get("message")
                .and_then(Value::as_str)
                .and_then(|message| {
                    let lower = message.to_ascii_lowercase();
                    let tail = lower.split("try again in").nth(1)?.trim_start();
                    let end = tail
                        .find(|c: char| !c.is_ascii_digit() && c != '.')
                        .unwrap_or(tail.len());
                    let number: f64 = tail[..end].parse().ok()?;
                    let unit = tail[end..].trim_start();
                    let seconds = if unit.starts_with("ms") {
                        number / 1000.0
                    } else if unit.starts_with('s') {
                        number
                    } else {
                        return None;
                    };
                    (seconds.is_finite() && seconds >= 0.0)
                        .then(|| now.saturating_add(seconds.ceil().min(i64::MAX as f64) as i64))
                })
        })
        .flatten();
    let window_reset = if kind == "quota_exhausted" {
        let family = headers
            .get("x-codex-active-limit")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("codex")
            .replace('_', "-");
        ["primary", "secondary"]
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
            .max()
    } else {
        None
    };
    // No invented quota reset: absent official timing means a short probe cooldown.
    let until = [reset, retry, message_retry, window_reset]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(now.saturating_add(60))
        .max(now.saturating_add(1));
    Some(SupplierFailure::Cooldown {
        kind,
        until,
        code: if code.is_empty() {
            "http_429".into()
        } else {
            code.into()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn quota_and_throttle_are_distinct_and_only_explicit_auth_is_permanent() {
        let h = HeaderMap::new();
        assert_eq!(
            classify_supplier_failure(
                Some(429),
                &json!({"error":{"type":"usage_limit_reached","resets_at":1000}}),
                &h,
                100
            ),
            Some(SupplierFailure::Cooldown {
                kind: "quota_exhausted",
                until: 1000,
                code: "usage_limit_reached".into()
            })
        );
        assert_eq!(
            classify_supplier_failure(
                None,
                &json!({"type":"response.failed","response":{"error":{"code":"rate_limit_exceeded","message":"Please try again in 11.054s."}}}),
                &h,
                100
            ),
            Some(SupplierFailure::Cooldown {
                kind: "rate_limited",
                until: 112,
                code: "rate_limit_exceeded".into()
            })
        );
        for code in [
            "invalid_prompt",
            "usage_not_included",
            "flex_unavailable",
            "cyber_policy",
        ] {
            assert!(
                classify_supplier_failure(Some(429), &json!({"error":{"code":code}}), &h, 100)
                    .is_none()
            );
        }
        for status in [403, 500, 502, 504] {
            assert!(classify_supplier_failure(Some(status), &Value::Null, &h, 100).is_none());
        }
    }
}

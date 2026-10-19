//! Google RPC error envelopes of Cloud Code Assist, normalized once for the shared
//! ledger and the public error allowlist, and the Antigravity recovery policy.
use crate::SupplierFailure;
use http::HeaderMap;
use serde_json::{Value, json};

const RETRY_INFO: &str = "type.googleapis.com/google.rpc.RetryInfo";
const ERROR_INFO: &str = "type.googleapis.com/google.rpc.ErrorInfo";
/// Google asks for a wait on every 429. A short wait is ordinary throttling for the
/// caller to retry; a longer one is this account's quota for the model until the reset.
const QUOTA_COOLDOWN_SECONDS: f64 = 7.0;

pub(crate) fn auth_failure(
    error: &codex2api_suppliers::antigravity::AntigravityAuthError,
) -> crate::ResponseFailure {
    use codex2api_suppliers::antigravity::AntigravityAuthError as E;
    let (status, code, message) = match error {
        E::RefreshRejected {
            status,
            code,
            message,
        } => return crate::ResponseFailure::new(Some(*status), code.as_deref(), Some(message)),
        E::TokenEndpoint { status, message } | E::Service { status, message } => {
            return crate::ResponseFailure::new(Some(*status), None, Some(message));
        }
        E::Http(error) if error.is_timeout() => {
            (504, "upstream_timeout", "Google 授权服务请求超时")
        }
        E::Http(_) => (502, "upstream_transport_error", "Google 授权服务连接失败"),
        E::MissingRefreshToken | E::TokensNotFound(_) => (
            503,
            "upstream_auth_missing",
            "Antigravity 供应账户没有可用授权",
        ),
        E::MissingProject => (
            503,
            "upstream_project_missing",
            "Antigravity 供应账户尚未完成 Cloud Code Assist 初始化",
        ),
        _ => (
            502,
            "upstream_auth_unavailable",
            "Antigravity 上游授权当前不可用",
        ),
    };
    crate::ResponseFailure::new(Some(status), Some(code), Some(message))
}

/// Go duration text such as `7s`, `0.5s`, `4m50s` or `250ms`.
pub(crate) fn duration_seconds(text: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut rest = text.trim();
    if rest.is_empty() {
        return None;
    }
    while !rest.is_empty() {
        let digits = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        let value: f64 = rest[..digits].parse().ok()?;
        rest = &rest[digits..];
        let unit = rest
            .find(|c: char| c.is_ascii_digit() || c == '.')
            .unwrap_or(rest.len());
        let scale = match &rest[..unit] {
            "h" => 3600.0,
            "m" => 60.0,
            "s" => 1.0,
            "ms" => 0.001,
            "us" | "µs" => 0.000_001,
            "ns" => 0.000_000_001,
            _ => return None,
        };
        total += value * scale;
        rest = &rest[unit..];
    }
    total.is_finite().then_some(total)
}

/// Fields read from Google's raw envelope, our normalized one, or a `response.failed`
/// event whose normalized error sits in `response.error`.
struct Parsed<'a> {
    status: Option<u16>,
    rpc_status: Option<&'a str>,
    reason: Option<&'a str>,
    model: Option<String>,
    retry_after: Option<f64>,
    message: Option<&'a str>,
}

fn parse(value: &Value) -> Parsed<'_> {
    let error = value
        .pointer("/response/error")
        .or_else(|| value.get("error"))
        .filter(|v| v.is_object())
        .unwrap_or(value);
    let details = error["details"].as_array();
    let detail = |kind: &str| details.into_iter().flatten().find(|d| d["@type"] == kind);
    let info = detail(ERROR_INFO);
    Parsed {
        status: [&value["status"], &error["status"], &error["code"]]
            .into_iter()
            .find_map(|status| status.as_u64().and_then(|n| u16::try_from(n).ok())),
        rpc_status: error["rpc_status"]
            .as_str()
            .or_else(|| error["status"].as_str()),
        reason: error["reason"]
            .as_str()
            .or_else(|| info.and_then(|d| d["reason"].as_str())),
        model: error["model"]
            .as_str()
            .or_else(|| info.and_then(|d| d["metadata"]["model"].as_str()))
            .map(|model| {
                model
                    .rsplit('/')
                    .next()
                    .unwrap_or(model)
                    .to_ascii_lowercase()
            }),
        retry_after: error["retry_after"].as_f64().or_else(|| {
            detail(RETRY_INFO).and_then(|d| duration_seconds(d["retryDelay"].as_str()?))
        }),
        message: error["message"].as_str().or_else(|| error.as_str()),
    }
}

/// The error body of a failed generation in the shape the shared layers expect.
/// Identifiers keep Google's status/reason vocabulary in lowercase; retry advice is
/// mirrored into `headers` so the caller receives a `Retry-After`.
pub fn normalize(status: u16, raw: &[u8]) -> Value {
    let value: Value = serde_json::from_slice(raw).unwrap_or_else(|_| {
        let text = String::from_utf8_lossy(raw);
        if text.trim_start().starts_with('<') {
            Value::Null
        } else {
            json!({"error":{"message":text.trim()}})
        }
    });
    let parsed = parse(&value);
    let code = parsed
        .reason
        .or(parsed.rpc_status)
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| format!("http_{status}"));
    let kind = match status {
        401 => "authentication_error",
        403 => "permission_error",
        429 => "rate_limit_error",
        400..=499 => "invalid_request_error",
        _ => "server_error",
    };
    let mut error = json!({"code":code,"type":kind,"status":status,
        "message":parsed.message.unwrap_or("Antigravity request failed")});
    if let Some(rpc) = parsed.rpc_status {
        error["rpc_status"] = rpc.into();
    }
    if let Some(reason) = parsed.reason {
        error["reason"] = reason.into();
    }
    if let Some(model) = &parsed.model {
        error["model"] = model.as_str().into();
    }
    let mut normalized = json!({"error":error,"status":status});
    if let Some(seconds) = parsed.retry_after {
        normalized["error"]["retry_after"] = seconds.into();
        normalized["headers"] = json!({"retry-after":seconds.ceil().max(1.0).to_string()});
    }
    normalized
}

/// Credential rejection and explicit model quota exhaustion remove supply; the
/// exhausted model alone cools down. Brief waits, 403 policy denials and shared
/// capacity shortages (503) are forwarded for the caller's own retry.
pub fn classify_failure(
    status: Option<u16>,
    value: &Value,
    _headers: &HeaderMap,
    now: i64,
) -> Option<SupplierFailure> {
    if status == Some(401) {
        return Some(SupplierFailure::Authentication);
    }
    let parsed = parse(value);
    let status = status.or(parsed.status);
    if status != Some(429) && parsed.rpc_status != Some("RESOURCE_EXHAUSTED") {
        return None;
    }
    let message = parsed.message.unwrap_or("").to_ascii_lowercase();
    let exhausted = parsed
        .retry_after
        .is_some_and(|seconds| seconds >= QUOTA_COOLDOWN_SECONDS)
        || message.contains("exhausted your capacity")
        || message.contains("quota exceeded");
    if !exhausted {
        return None;
    }
    let delay = parsed
        .retry_after
        .map(|seconds| seconds.ceil().min(i64::MAX as f64) as i64)
        .unwrap_or(60)
        .max(1);
    Some(SupplierFailure::QuotaExhausted {
        until: now.saturating_add(delay),
        code: parsed
            .reason
            .or(parsed.rpc_status)
            .unwrap_or("RESOURCE_EXHAUSTED")
            .to_ascii_lowercase(),
        model: parsed.model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn google(
        status: &str,
        reason: Option<&str>,
        delay: Option<&str>,
        model: Option<&str>,
    ) -> Value {
        let mut details = Vec::new();
        if let Some(delay) = delay {
            details.push(json!({"@type":RETRY_INFO,"retryDelay":delay}));
        }
        if let Some(reason) = reason {
            details.push(json!({"@type":ERROR_INFO,"reason":reason,"domain":"cloudcode-pa.googleapis.com","metadata":{"model":model.unwrap_or_default()}}));
        }
        json!({"error":{"code":429,"message":"You have exhausted your capacity on this model.","status":status,"details":details}})
    }

    #[test]
    fn go_durations_are_parsed() {
        for (text, seconds) in [
            ("7s", 7.0),
            ("0.5s", 0.5),
            ("4m50s", 290.0),
            ("250ms", 0.25),
            ("1h2m", 3720.0),
        ] {
            assert_eq!(duration_seconds(text), Some(seconds), "{text}");
        }
        assert_eq!(duration_seconds("later"), None);
    }

    #[test]
    fn long_model_waits_cool_the_model_and_short_waits_stay_with_the_caller() {
        let raw = google(
            "RESOURCE_EXHAUSTED",
            Some("RATE_LIMIT_EXCEEDED"),
            Some("4m50s"),
            Some("publishers/anthropic/models/claude-sonnet-4-6"),
        );
        let normalized = normalize(429, raw.to_string().as_bytes());
        assert_eq!(normalized["error"]["code"], "rate_limit_exceeded");
        assert_eq!(normalized["error"]["type"], "rate_limit_error");
        assert_eq!(normalized["headers"]["retry-after"], "290");
        // A mid-stream failure arrives as a response.failed event without an HTTP status.
        let event = json!({"type":"response.failed","response":{"status":"failed","error":normalized["error"]}});
        for (status, value) in [(Some(429), &raw), (Some(429), &normalized), (None, &event)] {
            assert_eq!(
                classify_failure(status, value, &HeaderMap::new(), 100),
                Some(SupplierFailure::QuotaExhausted {
                    until: 390,
                    code: "rate_limit_exceeded".into(),
                    model: Some("claude-sonnet-4-6".into()),
                })
            );
        }
        let brief = json!({"error":{"code":429,"message":"Resource has been exhausted (e.g. check quota).","status":"RESOURCE_EXHAUSTED",
            "details":[{"@type":RETRY_INFO,"retryDelay":"0.8s"},{"@type":ERROR_INFO,"reason":"RATE_LIMIT_EXCEEDED","metadata":{"model":"gemini-3-pro-high"}}]}});
        assert_eq!(
            classify_failure(Some(429), &brief, &HeaderMap::new(), 100),
            None
        );
        let capacity = json!({"error":{"code":503,"message":"The model is overloaded.","status":"UNAVAILABLE",
            "details":[{"@type":ERROR_INFO,"reason":"MODEL_CAPACITY_EXHAUSTED","metadata":{"model":"gemini-3-pro-high"}}]}});
        assert_eq!(
            classify_failure(Some(503), &capacity, &HeaderMap::new(), 100),
            None
        );
        assert_eq!(
            classify_failure(
                Some(403),
                &json!({"error":{"status":"PERMISSION_DENIED"}}),
                &HeaderMap::new(),
                100
            ),
            None
        );
        assert_eq!(
            classify_failure(Some(401), &Value::Null, &HeaderMap::new(), 100),
            Some(SupplierFailure::Authentication)
        );
    }

    #[test]
    fn html_and_plain_bodies_normalize_without_leaking_markup() {
        let html = normalize(502, b"<html>bad gateway</html>");
        assert_eq!(html["error"]["code"], "http_502");
        assert_eq!(html["error"]["message"], "Antigravity request failed");
        let plain = normalize(400, b"invalid project");
        assert_eq!(plain["error"]["message"], "invalid project");
        assert_eq!(plain["error"]["type"], "invalid_request_error");
    }
}

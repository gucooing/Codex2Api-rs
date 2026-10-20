//! Google RPC error envelopes of Cloud Code Assist, normalized once for the shared
//! ledger and the public error allowlist, and the Antigravity recovery policy.
use serde_json::{Value, json};

const RETRY_INFO: &str = "type.googleapis.com/google.rpc.RetryInfo";
const ERROR_INFO: &str = "type.googleapis.com/google.rpc.ErrorInfo";

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
pub(crate) struct Parsed<'a> {
    pub status: Option<u16>,
    pub rpc_status: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub model: Option<String>,
    pub retry_after: Option<f64>,
    pub message: Option<&'a str>,
}

/// Fields of a Google error wherever the channel placed them.
pub(crate) fn parsed(value: &Value) -> Parsed<'_> {
    parse(value)
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
            .or_else(|| info.and_then(|d| d["reason"].as_str()))
            .or_else(|| error["code"].as_str()),
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn google_envelopes_normalize_with_retry_advice() {
        let raw = json!({"error":{"code":429,"message":"You have exhausted your capacity on this model.","status":"RESOURCE_EXHAUSTED",
            "details":[{"@type":RETRY_INFO,"retryDelay":"4m50s"},{"@type":ERROR_INFO,"reason":"QUOTA_EXHAUSTED","metadata":{"model":"publishers/anthropic/models/claude-sonnet-4-6"}}]}});
        let normalized = normalize(429, raw.to_string().as_bytes());
        assert_eq!(normalized["error"]["code"], "quota_exhausted");
        assert_eq!(normalized["error"]["type"], "rate_limit_error");
        assert_eq!(normalized["headers"]["retry-after"], "290");
        let parsed = parsed(&normalized);
        assert_eq!(parsed.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(parsed.retry_after, Some(290.0));
    }
}

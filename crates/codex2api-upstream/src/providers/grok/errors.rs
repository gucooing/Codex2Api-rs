//! Grok Build's flat, nested, WKE and JSON-encoded provider error envelopes.
use serde_json::{Value, json};

pub(crate) const FREE_USAGE_EXHAUSTED: &str = "subscription:free-usage-exhausted";

pub(crate) fn auth_failure(
    error: &codex2api_suppliers::grok::GrokAuthError,
) -> crate::ResponseFailure {
    use codex2api_suppliers::grok::GrokAuthError as E;
    let (status, code, message) = match error {
        E::RefreshRejected {
            status,
            code,
            message,
            ..
        } => {
            return crate::ResponseFailure::new(Some(*status), code.as_deref(), Some(message));
        }
        E::TokenEndpoint { status, message } => {
            return crate::ResponseFailure::new(Some(*status), None, Some(message));
        }
        E::Http(error) if error.is_timeout() => (504, "upstream_timeout", "Grok 授权服务请求超时"),
        E::Http(_) => (502, "upstream_transport_error", "Grok 授权服务连接失败"),
        E::MissingRefreshToken | E::TokensNotFound(_) => {
            (503, "upstream_auth_missing", "Grok 供应账户没有可用授权")
        }
        _ => (502, "upstream_auth_unavailable", "Grok 上游授权当前不可用"),
    };
    crate::ResponseFailure::new(Some(status), Some(code), Some(message))
}

pub(crate) fn decode(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|_| {
        // Keep plain rejection text for classification, but never inspect an HTML edge page.
        let text = String::from_utf8_lossy(bytes);
        if text.trim_start().starts_with('<') {
            Value::Null
        } else {
            Value::String(text.into_owned())
        }
    })
}

pub(crate) fn status(value: &Value) -> Option<u16> {
    ["status", "status_code", "code"]
        .into_iter()
        .find_map(|key| {
            let status = value[key]
                .as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .or_else(|| value[key].as_str()?.trim().parse::<u16>().ok())?;
            (400..=599).contains(&status).then_some(status)
        })
}

pub fn normalize(value: &Value) -> Value {
    normalize_reply(value, None)
}

pub(super) fn normalize_reply(value: &Value, http_status: Option<u16>) -> Value {
    let mut error = normalize_at(value, 0);
    if status(&error).is_none()
        && let Some(status) = http_status
    {
        error["status"] = status.into();
    }
    if let Some(code) = super::scheduling::quota_code(status(&error), &error).map(str::to_owned) {
        if error["code"].is_null() {
            error["code"] = code.into();
        }
        // Keep Grok's diagnostic code while sharing only a protocol-neutral category.
        error["type"] = "quota_exhausted".into();
    }
    error
}

fn normalize_at(value: &Value, depth: usize) -> Value {
    if depth >= 8 {
        return json!({"message":"Grok request failed"});
    }
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .find(|item| item.is_object() || item.is_string())
            .map(|item| normalize_at(item, depth + 1))
            .unwrap_or_else(|| json!({"message":"Grok request failed"}));
    }
    let envelope = value
        .get("response")
        .filter(|v| v.get("error").is_some_and(|e| !e.is_null()))
        .unwrap_or(value);
    let error = envelope
        .get("error")
        .filter(|v| !v.is_null())
        .unwrap_or(envelope);
    if error.is_array() {
        let mut result = normalize_at(error, depth + 1);
        inherit(&mut result, envelope);
        inherit(&mut result, value);
        return result;
    }
    let message = error.as_str().or_else(|| {
        ["message", "error", "detail", "description", "msg"]
            .into_iter()
            .find_map(|key| error[key].as_str().filter(|s| !s.trim().is_empty()))
    });
    let mut result = if let Some(inner) = message
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .filter(|v| v.is_object() || v.is_array() || v.is_string())
    {
        normalize_at(&inner, depth + 1)
    } else {
        let mut result = if error.is_object() {
            error.clone()
        } else {
            json!({})
        };
        result["message"] = message.unwrap_or("Grok request failed").into();
        result
    };
    for source in [error, envelope, value] {
        inherit(&mut result, source);
    }
    if let Some(status) = status(&result) {
        result["status"] = status.into();
    }
    if let Some(code) = result["message"]
        .as_str()
        .and_then(|message| message.split_once("[WKE="))
        .and_then(|(_, tail)| tail.split_once(']'))
        .map(|(tag, _)| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty())
    {
        result["code"] = code.into();
    }
    result
}

/// Normalize errors before lifecycle accounting and public field filtering on either listener.
pub fn normalize_response(value: &mut Value) {
    if value
        .pointer("/response/error")
        .is_some_and(|v| !v.is_null())
    {
        value["response"]["error"] = normalize(value);
    } else if value.get("error").is_some_and(|v| !v.is_null()) || value["type"] == "error" {
        let error = normalize(value);
        value["error"] = error;
        if value["type"].is_null() {
            value["type"] = "error".into();
        }
    }
}

fn inherit(result: &mut Value, source: &Value) {
    if result["code"].is_null()
        && let Some(code) = source
            .get("code")
            .filter(|v| !v.is_null())
            .or_else(|| source.get("error_code"))
        && code.is_string()
    {
        result["code"] = code.clone();
    }
    if result["type"].is_null()
        && let Some(kind) = source["type"]
            .as_str()
            .filter(|kind| *kind != "error" && !kind.starts_with("response."))
    {
        result["type"] = kind.into();
    }
    if status(result).is_none()
        && let Some(status) = status(source)
    {
        result["status"] = status.into();
    }
    for key in [
        "retry_after",
        "retry_after_ms",
        "retry_after_seconds",
        "reset_at",
        "resets_at",
        "retry_at",
        "headers",
    ] {
        if result[key].is_null()
            && let Some(value) = source.get(key)
        {
            result[key] = value.clone();
        }
    }
}

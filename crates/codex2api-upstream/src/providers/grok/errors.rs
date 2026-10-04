//! Grok's flat and nested machine-readable error envelopes.
use serde_json::{Value, json};
pub fn normalize(value: &Value) -> Value {
    let error = value
        .pointer("/response/error")
        .or_else(|| value.get("error"))
        .unwrap_or(value);
    let message = error
        .as_str()
        .or_else(|| error["message"].as_str())
        .unwrap_or("Grok request failed");
    let wke = message
        .split_once("[WKE=")
        .and_then(|(_, tail)| tail.split_once(']').map(|(tag, _)| tag));
    let code = wke
        .or_else(|| error["code"].as_str())
        .or_else(|| value["code"].as_str());
    let mut result = if error.is_object() {
        error.clone()
    } else {
        json!({"message":message})
    };
    if let Some(code) = code {
        result["code"] = code.into();
    }
    for key in [
        "status",
        "status_code",
        "retry_after",
        "retry_after_ms",
        "retry_after_seconds",
        "reset_at",
        "resets_at",
    ] {
        if result[key].is_null() && value[key].is_number() {
            result[key] = value[key].clone();
        }
    }
    if result["status"].is_null() && value["code"].is_number() {
        result["status"] = value["code"].clone();
    }
    result
}

//! Supplier identity and credentials never reach a caller. Protocol envelopes are
//! stripped of supplier metadata; error messages are redacted; model output and
//! opaque tool data pass untouched.
use http::HeaderMap;
use serde_json::{Map, Value};

/// Reply headers a caller may receive.
pub fn headers(headers: &mut HeaderMap) {
    let remove = headers
        .keys()
        .filter(|name| {
            !matches!(
                name.as_str(),
                "content-type"
                    | "content-encoding"
                    | "content-length"
                    | "cache-control"
                    | "retry-after"
                    | "retry-after-ms"
                    | "x-should-retry"
                    | "x-request-id"
                    | "x-oai-request-id"
                    | "openai-model"
                    | "x-openai-model"
                    | "x-codex-models-etag"
                    | "x-codex-turn-state"
                    | "x-codex-plan-type"
                    | "x-codex-credits-has-credits"
                    | "x-codex-credits-unlimited"
                    | "x-codex-primary-used-percent"
                    | "x-codex-primary-reset-at"
                    | "x-codex-primary-reset-after-seconds"
                    | "x-codex-primary-window-minutes"
                    | "x-codex-secondary-used-percent"
                    | "x-codex-secondary-reset-at"
                    | "x-codex-secondary-reset-after-seconds"
                    | "x-codex-secondary-window-minutes"
                    | "location"
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    for name in remove {
        headers.remove(name);
    }
    headers.insert(http::header::CACHE_CONTROL, "no-store".parse().unwrap());
}

/// A supplier message a caller may read: bounded, without secrets, e-mail addresses
/// or supplier organization, project, user or workspace identifiers.
pub fn text(message: &str) -> String {
    crate::request_failure::diagnostic_text(message, 512)
        .split(' ')
        .map(|word| {
            let bare = word
                .trim_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_ascii_lowercase();
            let private = word.contains('@')
                || [
                    "org-",
                    "org_",
                    "proj_",
                    "user-",
                    "user_",
                    "acct_",
                    "workspace-",
                ]
                .iter()
                .any(|prefix| bare.starts_with(prefix));
            if private { "[REDACTED]" } else { word }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Keep HTTP retry advice without exposing arbitrary supplier diagnostic headers.
fn retry_headers(value: &Value) -> Option<Value> {
    let mut headers = HeaderMap::new();
    let mut entries = value.as_object()?.iter().collect::<Vec<_>>();
    entries.sort_by_key(|(name, _)| *name);
    for (name, value) in entries {
        let Ok(name) = http::HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        if !matches!(
            name.as_str(),
            "retry-after" | "retry-after-ms" | "x-should-retry"
        ) {
            continue;
        }
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Number(_) | Value::Bool(_) => value.to_string(),
            _ => continue,
        };
        if let Ok(value) = http::HeaderValue::from_str(&text) {
            headers.insert(name, value);
        }
    }
    let mapped: Map<String, Value> = headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.to_string(), value.into()))
        })
        .collect();
    (!mapped.is_empty()).then_some(Value::Object(mapped))
}

const PRIVATE_FIELDS: [&str; 61] = [
    "supplier_account_id",
    "supplier_id",
    "supplier_account",
    "supplier_tokens",
    "account",
    "accounts",
    "account_name",
    "account_profile",
    "organization_id",
    "organization",
    "organizations",
    "organization_name",
    "workspace_name",
    "workspace_id",
    "workspace",
    "workspaces",
    "chatgpt_account_id",
    "chatgpt_user_id",
    "account_id",
    "account_user_id",
    "user_id",
    "user",
    "email",
    "username",
    "display_name",
    "displayname",
    "accountid",
    "userid",
    "installation_id",
    "x-codex-installation-id",
    "access_token",
    "refresh_token",
    "id_token",
    "api_key",
    "apikey",
    "accesstoken",
    "refreshtoken",
    "idtoken",
    "installationid",
    "authorization",
    "cookie",
    "cookies",
    "credentials",
    "auth_json",
    "auth",
    "fingerprint",
    "http_fingerprint",
    "http_fingerprint_json",
    "proxy_id",
    "rate_limits",
    "rate_limit",
    "credits",
    "plan_type",
    "chatgpt_plan_type",
    "teamid",
    "team_id",
    "teamname",
    "principalid",
    "principaltype",
    "organizationid",
    "organizationname",
];

/// An error object a caller may read: its type, code, parameter and redacted message,
/// plus retry advice.
pub fn error(error: &Value) -> Value {
    let Some(object) = error.as_object() else {
        return serde_json::json!({"message":text(error.as_str().unwrap_or("The supplier request failed."))});
    };
    let mut safe = Map::new();
    for key in [
        "type",
        "code",
        "param",
        "status",
        "status_code",
        "retry_after",
        "retry_after_seconds",
        "retry_after_ms",
        "resets_at",
        "reset_at",
        "retry_at",
        "resets_in_seconds",
    ] {
        if let Some(value) = object.get(key).filter(|v| !v.is_null()) {
            safe.insert(key.into(), value.clone());
        }
    }
    if let Some(message) = object.get("message").and_then(Value::as_str) {
        safe.insert("message".into(), text(message).into());
    }
    if let Some(headers) = object.get("headers").and_then(retry_headers) {
        safe.insert("headers".into(), headers);
    }
    Value::Object(safe)
}

/// Sanitize protocol envelopes, never model output or opaque tool/search data.
/// Callers restore their own request metadata after this supplier boundary.
/// Returns whether sanitization changed the event, avoiding a full comparison copy.
pub fn metadata(value: &mut Value) -> bool {
    let mut changed = false;
    match value {
        Value::Array(items) => {
            for item in items {
                changed |= metadata(item);
            }
        }
        Value::Object(object) => {
            let response_output = object
                .get("type")
                .and_then(Value::as_str)
                .is_none_or(|kind| kind.starts_with("response."));
            let error_event = object.get("type").and_then(Value::as_str) == Some("error");
            if error_event && let Some(headers) = object.get("headers") {
                let safe = retry_headers(headers);
                if safe.as_ref() != Some(headers) {
                    changed = true;
                    object.remove("headers");
                    if let Some(safe) = safe {
                        object.insert("headers".into(), safe);
                    }
                }
            }
            let original_len = object.len();
            object.retain(|key, _| {
                let lowercase;
                let name = if key.bytes().any(|b| b.is_ascii_uppercase()) {
                    lowercase = key.to_ascii_lowercase();
                    lowercase.as_str()
                } else {
                    key.as_str()
                };
                !PRIVATE_FIELDS.contains(&name)
            });
            changed |= original_len != object.len();
            for (key, item) in object.iter_mut() {
                if key == "error" && !item.is_null() {
                    let safe = error(item);
                    if *item != safe {
                        *item = safe;
                        changed = true;
                    }
                } else if matches!(
                    key.as_str(),
                    "content"
                        | "text"
                        | "arguments"
                        | "input"
                        | "tools"
                        | "tool_choice"
                        | "results"
                        | "data"
                        | "annotations"
                        | "logprobs"
                        | "audio"
                        | "refusal"
                        | "summary"
                        | "delta"
                        | "parts"
                ) {
                    // These are application payloads. A JSON field called `user`
                    // in a tool result is not the supplier's account identity.
                    continue;
                } else if key == "output" {
                    // Responses output is a list of protocol items. A tool item's
                    // own `output` is opaque content, including structured JSON.
                    if response_output && let Value::Array(items) = item {
                        for output in items {
                            if output.get("type").and_then(Value::as_str).is_some() {
                                changed |= metadata(output);
                            }
                        }
                    }
                } else {
                    changed |= metadata(item);
                }
            }
            if error_event && let Some(message) = object.get("message").and_then(Value::as_str) {
                let safe = text(message);
                if safe != message {
                    object.insert("message".into(), safe.into());
                    changed = true;
                }
            }
        }
        _ => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn protocol_envelopes_are_stripped_but_application_payloads_are_not() {
        let payload = json!({"user":"customer","account":{"email":"shown@example.test"},"metadata":{"api_key":"example"},"error":{"message":"quoted"}});
        let mut value = json!({"account_id":"supplier-secret","response":{"user":"supplier-secret","output":[{"type":"message","content":[{"type":"output_text","text":"text","annotations":[payload.clone()]}]},{"type":"function_call_output","output":payload.clone()}]},"results":[payload.clone()],"data":[payload.clone()]});
        metadata(&mut value);
        assert!(value.get("account_id").is_none());
        assert!(value["response"].get("user").is_none());
        assert_eq!(
            value["response"]["output"][0]["content"][0]["annotations"][0],
            payload
        );
        assert_eq!(value["response"]["output"][1]["output"], payload);
        assert_eq!(value["results"][0], payload);
        assert_eq!(value["data"][0], payload);
    }
    #[test]
    fn errors_keep_their_identity_but_not_supplier_details() {
        let mut value = json!({"type":"response.failed","response":{"id":"request-owned","error":{"type":"rate_limit_exceeded","code":"slow_down","message":"secret@example.test supplier-account org-123","retry_after_ms":1200,"debug":{"refresh_token":"supplier-secret"}},"metadata":{"account_id":"supplier-account","installation_id":"supplier-installation"}}});
        metadata(&mut value);
        let raw = value.to_string();
        assert!(!raw.contains("supplier-secret"));
        assert!(!raw.contains("secret@"));
        assert!(!raw.contains("org-123"));
        assert_eq!(value["response"]["error"]["code"], "slow_down");
        assert_eq!(value["response"]["error"]["retry_after_ms"], 1200);
        assert_eq!(value["response"]["id"], "request-owned");
    }
}

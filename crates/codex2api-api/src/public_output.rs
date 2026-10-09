//! Supplier diagnostics and identity metadata are never part of a consumer response.
use axum::http::HeaderMap;
use serde_json::{Value, json};

pub(crate) fn headers(headers: &mut HeaderMap) {
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
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        "no-store".parse().unwrap(),
    );
}
fn identifier(value: &Value) -> Option<&str> {
    value.as_str().filter(|v| {
        !v.is_empty()
            && v.len() <= 80
            && v.bytes()
                .all(|c| c.is_ascii_lowercase() || b"_-:.".contains(&c) || c.is_ascii_digit())
    })
}
pub(crate) fn error(value: &Value) -> Value {
    let mut safe = json!({"message":"The service could not complete this request."});
    for key in ["code", "type"] {
        if let Some(text) = identifier(&value[key]) {
            safe[key] = text.into();
        }
    }
    for key in [
        "status",
        "status_code",
        "retry_after",
        "retry_after_seconds",
        "retry_after_ms",
        "resets_at",
        "reset_at",
        "retry_at",
    ] {
        if value[key].is_number() {
            safe[key] = value[key].clone();
        }
    }
    if let Some(headers) = retry_headers(&value["headers"]) {
        safe["headers"] = headers;
    }
    safe
}

/// Keep HTTP retry advice without exposing arbitrary supplier diagnostic headers.
fn retry_headers(value: &Value) -> Option<Value> {
    let mut headers = HeaderMap::new();
    // Official JSON header maps are ordered by key before duplicate HTTP names collapse.
    let mut entries = value.as_object()?.iter().collect::<Vec<_>>();
    entries.sort_by_key(|(name, _)| *name);
    for (name, value) in entries {
        let Ok(name) = http::HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        if !matches!(name.as_str(), "retry-after" | "retry-after-ms") {
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
    let mapped: serde_json::Map<String, Value> = headers
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
pub(crate) fn metadata(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                metadata(item)
            }
        }
        Value::Object(object) => {
            if matches!(
                object.get("type").and_then(Value::as_str),
                Some("error" | "response.failed")
            ) && let Some(headers) = object.remove("headers")
                && let Some(headers) = retry_headers(&headers)
            {
                object.insert("headers".into(), headers);
            }
            object.retain(|key, _| {
                !matches!(
                    key.to_ascii_lowercase().as_str(),
                    "supplier_account_id"
                        | "supplier_id"
                        | "supplier_account"
                        | "supplier_tokens"
                        | "account"
                        | "accounts"
                        | "account_name"
                        | "account_profile"
                        | "organization_id"
                        | "organization"
                        | "organizations"
                        | "organization_name"
                        | "workspace_name"
                        | "workspace_id"
                        | "workspace"
                        | "workspaces"
                        | "chatgpt_account_id"
                        | "chatgpt_user_id"
                        | "account_id"
                        | "account_user_id"
                        | "user_id"
                        | "user"
                        | "email"
                        | "username"
                        | "display_name"
                        | "displayname"
                        | "accountid"
                        | "userid"
                        | "installation_id"
                        | "x-codex-installation-id"
                        | "access_token"
                        | "refresh_token"
                        | "id_token"
                        | "api_key"
                        | "apikey"
                        | "accesstoken"
                        | "refreshtoken"
                        | "idtoken"
                        | "installationid"
                        | "authorization"
                        | "cookie"
                        | "cookies"
                        | "credentials"
                        | "auth_json"
                        | "auth"
                        | "fingerprint"
                        | "http_fingerprint"
                        | "http_fingerprint_json"
                        | "proxy_id"
                        | "rate_limits"
                        | "rate_limit"
                        | "credits"
                        | "plan_type"
                        | "chatgpt_plan_type"
                        | "teamid"
                        | "team_id"
                        | "teamname"
                        | "principalid"
                        | "principaltype"
                        | "organizationid"
                        | "organizationname"
                        | "cost_in_usd_ticks"
                )
            });
            for (key, item) in object.iter_mut() {
                if key == "error" && !item.is_null() {
                    *item = error(item);
                } else {
                    metadata(item);
                }
            }
            if object.get("type").and_then(Value::as_str) == Some("error") {
                object.insert(
                    "message".into(),
                    json!("The service could not complete this request."),
                );
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn errors_and_metadata_do_not_expose_supplier_fields_or_raw_messages() {
        let mut value = json!({"type":"response.failed","response":{"id":"request-owned","error":{"type":"rate_limit_exceeded","code":"slow_down","message":"secret@example.test supplier-account","retry_after_ms":1200,"debug":{"refresh_token":"supplier-secret"}},"metadata":{"account_id":"supplier-account","installation_id":"supplier-installation"}}});
        metadata(&mut value);
        let raw = value.to_string();
        assert!(!raw.contains("supplier-"));
        assert!(!raw.contains("secret@"));
        assert_eq!(value["response"]["error"]["code"], "slow_down");
        assert_eq!(value["response"]["error"]["retry_after_ms"], 1200);
        assert_eq!(value["response"]["id"], "request-owned");
    }

    #[test]
    fn retry_headers_preserve_both_locations_and_validate_http_values() {
        for (nested, expected) in [
            (json!({"Retry-After":7}), json!({"retry-after":"7"})),
            (
                json!({"retry-after":"Wed, 21 Oct 2026 07:28:00 GMT"}),
                json!({"retry-after":"Wed, 21 Oct 2026 07:28:00 GMT"}),
            ),
            (
                json!({"retry-after":"30","Retry-After":"5"}),
                json!({"retry-after":"30"}),
            ),
            (json!({"retry-after":"\n5\n"}), Value::Null),
            (json!({"retry-after":["5"]}), Value::Null),
            (json!("invalid"), Value::Null),
        ] {
            let mut event = json!({"type":"error","status":429,"error":{"code":"rate_limit_exceeded","headers":nested,"message":"supplier-secret"},"headers":{"Retry-After":"12","Authorization":"supplier-secret","Set-Cookie":"supplier-secret"}});
            metadata(&mut event);
            assert_eq!(event["error"]["headers"], expected);
            assert_eq!(event["headers"], json!({"retry-after":"12"}));
            assert_eq!(event["status"], 429);
            assert!(!event.to_string().contains("supplier-secret"));
        }
    }
}

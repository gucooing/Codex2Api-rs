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
                .all(|c| c.is_ascii_lowercase() || c == b'_' || c.is_ascii_digit())
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
    safe
}
pub(crate) fn metadata(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                metadata(item)
            }
        }
        Value::Object(object) => {
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
}

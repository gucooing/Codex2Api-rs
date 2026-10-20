//! Supplier diagnostics and identity metadata are never part of a consumer response.
use http::HeaderMap;
use serde_json::{Value, json};

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
fn identifier(value: &Value) -> Option<&str> {
    value.as_str().filter(|v| is_identifier(v))
}
/// Statuses a caller error keeps. Authentication, billing, permission and throttling
/// statuses drive client credential and backoff behavior, so they never describe it.
fn caller_status(status: u16) -> bool {
    matches!(status, 400 | 404 | 409 | 413 | 415 | 422)
}
fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 80
        && text
            .bytes()
            .all(|c| c.is_ascii_lowercase() || b"_-:.".contains(&c) || c.is_ascii_digit())
}

const UNAVAILABLE: &str = "The service is temporarily unavailable. Please try again later.";

/// What a consumer is told about a supplier failure once the gateway has handled it.
/// Errors the caller can correct keep their code, parameter and a redacted message,
/// and throttling keeps its retry advice. Supplier credentials, billing, quota,
/// permissions and capacity are the service's own state: the consumer only learns
/// that the service is temporarily unavailable. Local consumer errors (keys, wallets,
/// subscriptions, limits, validation) are already in consumer terms and never pass
/// through this policy. Applying it to its own output changes nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicFailure {
    pub status: u16,
    pub code: String,
    pub message: String,
    disposition: Disposition,
    /// The supplier's error type and parameter, kept only for caller errors.
    error_type: Option<String>,
    param: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Disposition {
    /// The caller can correct the request.
    Caller,
    /// The caller backs off and retries with the original advice.
    Throttled,
    /// The service's own state; nothing about it is described.
    Service,
}

impl PublicFailure {
    pub fn new(failure: &crate::ResponseFailure) -> Self {
        use crate::FailureKind as K;
        if failure.kind == K::QuotaExhausted {
            return Self::unavailable();
        }
        let code = failure.code.as_deref();
        match code {
            // The gateway's own continuation contract, in the official client's vocabulary.
            Some("supplier_context_required") => {
                return Self::caller(
                    400,
                    "previous_response_not_found",
                    "The previous response is no longer available. Send the full conversation without previous_response_id.".into(),
                );
            }
            Some("supplier_concurrency_exceeded") => {
                return Self::throttling(429, "rate_limit_exceeded", None);
            }
            Some(
                "supplier_pool_exhausted"
                | "supplier_model_unavailable"
                | "supplier_workspace_changed",
            ) => {
                return Self::unavailable();
            }
            // Content and context rejections describe the caller's own input.
            Some(
                code @ ("context_length_exceeded"
                | "invalid_prompt"
                | "cyber_policy"
                | "bio_policy"
                | "misalignment_policy_violation"
                | "prompt_blocked"),
            ) => {
                return Self::caller(
                    failure
                        .status
                        .filter(|status| caller_status(*status))
                        .unwrap_or(400),
                    code,
                    failure
                        .message
                        .as_deref()
                        .map(public_text)
                        .filter(|message| !message.is_empty())
                        .unwrap_or_else(|| "The request was rejected.".into()),
                );
            }
            Some(code @ ("rate_limit_exceeded" | "slow_down")) => {
                let status = match failure.status {
                    Some(status @ (429 | 503)) => status,
                    _ => 429,
                };
                return Self::throttling(status, code, failure.message.as_deref());
            }
            _ => {}
        }
        match failure.kind {
            K::InvalidRequest => Self::caller(
                failure
                    .status
                    .filter(|status| caller_status(*status))
                    .unwrap_or(400),
                code.filter(|code| is_identifier(code))
                    .unwrap_or("invalid_request_error"),
                failure
                    .message
                    .as_deref()
                    .map(public_text)
                    .filter(|message| !message.is_empty())
                    .unwrap_or_else(|| "The request is invalid.".into()),
            ),
            K::RateLimit | K::LimitUnknown => {
                Self::throttling(429, "rate_limit_exceeded", failure.message.as_deref())
            }
            K::Authentication | K::PaymentRequired | K::Permission | K::QuotaExhausted => {
                Self::unavailable()
            }
            K::Upstream if failure.status == Some(503) => Self::unavailable(),
            K::Timeout => Self::service(
                504,
                "upstream_timeout",
                "The upstream service timed out. Please try again later.",
            ),
            K::Transport | K::Upstream => Self::service(
                502,
                "upstream_error",
                "The upstream service could not complete this request.",
            ),
            // An incomplete response reaches the caller as a response with its reason.
            K::Incomplete => Self::caller(
                200,
                code.filter(|code| is_identifier(code))
                    .unwrap_or("response_incomplete"),
                "The response ended before it was complete.".into(),
            ),
        }
    }

    /// The consumer's view of a supplier error object or envelope; `http_status` is
    /// the supplier response status when the error arrived as an HTTP reply.
    pub fn from_error(http_status: Option<u16>, value: &Value) -> Self {
        let failure = crate::ResponseFailure::from_error(http_status, value);
        let error = value
            .pointer("/response/error")
            .or_else(|| value.get("error"))
            .filter(|error| error.is_object())
            .unwrap_or(value);
        let mut public = Self::new(&failure);
        if public.disposition == Disposition::Caller {
            public.error_type = identifier(&error["type"])
                .filter(|kind| *kind != "error")
                .map(str::to_owned);
            public.param = error["param"]
                .as_str()
                .filter(|param| {
                    !param.is_empty()
                        && param.len() <= 128
                        && param
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"_.[]-".contains(&c))
                })
                .map(str::to_owned);
        }
        public
    }

    pub fn status_code(&self) -> http::StatusCode {
        http::StatusCode::from_u16(self.status).unwrap_or(http::StatusCode::BAD_GATEWAY)
    }

    /// Whether the consumer receives the original retry advice with this failure.
    pub fn throttled(&self) -> bool {
        self.disposition == Disposition::Throttled
    }

    /// The error object in the OpenAI shape both listeners send.
    pub fn error(&self) -> Value {
        let error_type = self.error_type.as_deref().unwrap_or(match self.status {
            401 => "authentication_error",
            403 => "permission_error",
            429 => "rate_limit_error",
            400..=499 => "invalid_request_error",
            _ => "server_error",
        });
        json!({"type":error_type,"code":self.code,"message":self.message,"param":self.param})
    }

    fn caller(status: u16, code: &str, message: String) -> Self {
        Self::with(Disposition::Caller, status, code, message)
    }

    fn throttling(status: u16, code: &str, message: Option<&str>) -> Self {
        Self::with(
            Disposition::Throttled,
            status,
            code,
            throttle_message(message),
        )
    }

    /// Supplier credentials, billing, quota, permissions or capacity prevented service.
    pub fn unavailable() -> Self {
        Self::service(503, "service_unavailable", UNAVAILABLE)
    }

    /// A fault of this service.
    pub fn internal() -> Self {
        Self::service(
            500,
            "internal_error",
            "The service could not complete this request.",
        )
    }

    fn service(status: u16, code: &str, message: &str) -> Self {
        Self::with(Disposition::Service, status, code, message.into())
    }

    fn with(disposition: Disposition, status: u16, code: &str, message: String) -> Self {
        Self {
            status,
            code: code.into(),
            message,
            disposition,
            error_type: None,
            param: None,
        }
    }
}

/// Keep only the wait an official client reads from a throttling message.
fn throttle_message(message: Option<&str>) -> String {
    let wait = message.and_then(|message| {
        let lower = message.to_ascii_lowercase();
        let start = lower.find("try again in")? + "try again in".len();
        let rest = lower[start..].trim_start();
        let digits = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        rest[..digits]
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())?;
        let unit = match rest[digits..].trim_start() {
            unit if unit.starts_with("ms") => "ms",
            unit if unit.starts_with('s') => "s",
            _ => return None,
        };
        Some(format!("{}{unit}", &rest[..digits]))
    });
    match wait {
        Some(wait) => format!("Rate limit reached. Please try again in {wait}."),
        None => "Rate limit reached. Please try again later.".into(),
    }
}

/// A supplier message a consumer may read: bounded and redacted, without e-mail
/// addresses or supplier organization, project, user or workspace identifiers.
fn public_text(message: &str) -> String {
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

/// A supplier error object or envelope rewritten as the consumer's [`PublicFailure`].
pub fn error(value: &Value) -> Value {
    failure(None, value).1
}

/// The consumer's view of a supplier error object or envelope, with the error object
/// the consumer receives. An envelope's status takes part in the decision.
pub fn failure(http_status: Option<u16>, value: &Value) -> (PublicFailure, Value) {
    let public = PublicFailure::from_error(http_status, value);
    let error = value
        .pointer("/response/error")
        .or_else(|| value.get("error"))
        .filter(|error| error.is_object())
        .unwrap_or(value);
    let safe = rewrite(&public, error);
    (public, safe)
}

/// Numeric status fields carry the public status; retry advice survives only throttling.
fn rewrite(public: &PublicFailure, error: &Value) -> Value {
    let mut safe = public.error();
    if public.throttled() {
        for key in [
            "retry_after",
            "retry_after_seconds",
            "retry_after_ms",
            "resets_at",
            "reset_at",
            "retry_at",
        ] {
            if error[key].is_number() {
                safe[key] = error[key].clone();
            }
        }
        if let Some(headers) = retry_headers(&error["headers"]) {
            safe["headers"] = headers;
        }
    }
    for key in ["status", "status_code"] {
        if error[key].is_number() {
            safe[key] = public.status.into();
        }
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
            let public = failure_event(object);
            let error_event = object.get("type").and_then(Value::as_str) == Some("error");
            if let Some(public) = &public
                && let Some(headers) = object.get("headers")
            {
                let safe = public.throttled().then(|| retry_headers(headers)).flatten();
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
                !matches!(
                    name,
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
            changed |= original_len != object.len();
            for (key, item) in object.iter_mut() {
                if key == "error" && !item.is_null() {
                    // An error event's own status decides its nested error object too.
                    let safe = match &public {
                        Some(public) if error_event && item.is_object() => rewrite(public, item),
                        _ => error(item),
                    };
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
            if let Some(public) = public
                && object.get("type").and_then(Value::as_str) == Some("error")
            {
                // Stream error events carry the failure at the top level; WebSocket
                // error frames also carry the status the official client acts on.
                let mut fields = vec![("message", Value::from(public.message.clone()))];
                if object.contains_key("code") {
                    fields.push(("code", public.code.clone().into()));
                }
                if object.contains_key("param") {
                    fields.push(("param", public.param.clone().into()));
                }
                if object.get("status").is_some_and(Value::is_number) {
                    fields.push(("status", public.status.into()));
                }
                for (key, value) in fields {
                    if object.get(key) != Some(&value) {
                        object.insert(key.into(), value);
                        changed = true;
                    }
                }
            }
        }
        _ => {}
    }
    changed
}

/// The public view of a supplier failure event, read before its fields are rewritten.
fn failure_event(object: &serde_json::Map<String, Value>) -> Option<PublicFailure> {
    match object.get("type").and_then(Value::as_str)? {
        "error" => Some(PublicFailure::from_error(
            None,
            &match object.get("error").filter(|error| error.is_object()) {
                Some(error) => json!({"status":object.get("status"),"error":error}),
                None => json!({"error":{
                    "status":object.get("status"),
                    "code":object.get("code"),
                    "type":object.get("type").filter(|kind| *kind != "error"),
                    "message":object.get("message"),
                    "param":object.get("param"),
                }}),
            },
        )),
        "response.failed" => {
            let response = object.get("response")?;
            let error = response.get("error").filter(|error| error.is_object())?;
            Some(PublicFailure::from_error(
                None,
                &json!({"status_code":response.get("status_code"),"error":error}),
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn api_rewrite_protocol_envelopes_do_not_rewrite_application_payloads() {
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

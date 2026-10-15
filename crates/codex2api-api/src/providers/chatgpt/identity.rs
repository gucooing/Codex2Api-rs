#[cfg(test)]
mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
#[cfg(test)]
use account_fixture::AccountFixture;
use axum::{
    Json,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use codex2api_storage::{PlatformAccount, Storage};
use serde_json::{Value, json};

pub fn identity(account: &PlatformAccount) -> Value {
    json!({"id":account.id,"account_id":account.id,"user_id":format!("user-{}",account.id),"email":account.email,"name":account.name,"plan_type":account.plan_type.as_str(),"is_default":true})
}
pub fn account_check(account: &PlatformAccount) -> Value {
    let active = account.subscription_active_at(chrono::Utc::now().timestamp());
    let mut details = identity(account);
    details["account_user_id"] = format!("user-{}", account.id).into();
    details["structure"] = "personal".into();
    details["is_deactivated"] = false.into();
    json!({"accounts":{account.id.clone():{"account":details,"entitlement":{"subscription_plan":account.plan_type.as_str(),"has_active_subscription":active,"expires_at":account.subscription_expires_at},"features":[],"permissions":[]}},"account_ordering":[account.id],"user":identity(account)})
}

pub fn optimized_account_check(account: &PlatformAccount) -> Value {
    // The desktop consumes the selected account directly here, unlike the keyed v4 response.
    let mut selected = account_check(account)["accounts"][&account.id].clone();
    selected["account_user"] = json!({
        "id":format!("user-{}",account.id),"user_id":format!("user-{}",account.id),"account_id":account.id,
        // Virtual accounts do not have an upstream workspace seat or a pending upgrade request.
        "seat_type":null,"trial_expires_at":null,"pending_seat_upgrade_request":false
    });
    selected
}

pub fn workspace_check(account: &PlatformAccount) -> Value {
    // WHAM workspace discovery is an array; ChatGPT account checks are keyed by account ID.
    // Official native and Desktop readers both support this sentinel. It keeps
    // the client's configured service origin instead of routing virtual tokens
    // to chatgpt.com. Native workspace discovery requires an HTTPS base URL.
    let mut workspace = identity(account);
    workspace["workspace_backend_origin"] = "NO_CONSTRAINT".into();
    workspace["account_routing_override"] = "NO_CONSTRAINT".into();
    workspace["account_user_id"] = format!("user-{}", account.id).into();
    workspace["account_user_role"] = "account-owner".into();
    workspace["is_zdr"] = false.into();
    workspace["is_openai_internal"] = false.into();
    workspace["structure"] = "personal".into();
    json!({"accounts":[workspace],"default_account_id":account.id,"account_ordering":[account.id]})
}
pub fn subscriptions(account: &PlatformAccount) -> Value {
    json!({"id":format!("subscription-{}",account.id),"account_id":account.id,"plan_type":account.plan_type.as_str(),"active_until":account.subscription_expires_at,"will_renew":false})
}
pub fn json_response(value: Value) -> Response {
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(value),
    )
        .into_response()
}

pub(crate) async fn model_catalog(
    storage: &Storage,
    owner: &str,
    value: &mut Value,
) -> crate::Result<()> {
    let models = storage
        .available_virtual_models(owner, codex2api_core::CHATGPT)
        .await?;
    let allowed: std::collections::BTreeSet<_> = models
        .iter()
        .filter(|m| m.kind == "text")
        .map(|m| m.model.as_str())
        .collect();
    let permits = |value: &Value| value.as_str().is_some_and(|slug| allowed.contains(slug));
    let ultrafast = super::model_tiers::ultrafast_models(storage).await?;
    if let Some(models) = value["models"].as_array_mut() {
        models.retain(|m| permits(&m["slug"]));
    }
    // Desktop Icn falls back to model slug/title/description when no version
    // groups exist. Access comes solely from the provider catalog and plan.
    let entries = value["models"]
        .as_array_mut()
        .ok_or_else(|| crate::ApiError::internal("Invalid model catalog"))?;
    for model in models.iter().filter(|m| m.kind == "text") {
        if !entries.iter().any(|entry| entry["slug"] == model.model) {
            entries.push(json!({"slug":model.model,"title":model.model,"description":""}));
        }
    }
    if value.get("default_model_slug").is_some() && !permits(&value["default_model_slug"]) {
        value["default_model_slug"] = value["models"][0]["slug"].clone();
    }
    if let Some(versions) = value["versions"].as_array_mut() {
        for version in versions.iter_mut() {
            if let Some(slugs) = version["slugs"].as_array_mut() {
                slugs.retain(permits);
            }
            if let Some(presets) = version["intelligence_presets"].as_array_mut() {
                presets.retain(|p| permits(&p["model_slug"]));
            }
        }
        versions.retain(|v| v["slugs"].as_array().is_some_and(|s| !s.is_empty()));
    }
    if let Some(categories) = value["categories"].as_array_mut() {
        for category in categories.iter_mut() {
            if let Some(models) = category["supported_models"].as_array_mut() {
                models.retain(permits);
            }
        }
        categories.retain(|v| permits(&v["default_model"]));
    }
    if let Some(sliders) = value["slider_settings"].as_array_mut() {
        sliders.retain(|v| permits(&v["model_slug"]));
    }
    if let Some(groups) = value["internal_groups"].as_array_mut() {
        for group in groups.iter_mut() {
            if let Some(ids) = group["model_ids"].as_array_mut() {
                ids.retain(permits);
            }
        }
        groups.retain(|g| g["model_ids"].as_array().is_some_and(|ids| !ids.is_empty()));
    }
    // Desktop uses the first populated version before falling back to models.
    // Keep verified presets, then add basic choices for every otherwise omitted
    // model. A basic choice asserts only its name, never a reasoning capability.
    if !allowed.is_empty()
        && (value["versions"].as_array().is_some_and(|a| !a.is_empty())
            || value["categories"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
            || allowed.iter().any(|model| ultrafast.contains(*model)))
    {
        let mut presets: Vec<Value> = value["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|version| version["disabled"] != true)
            .flat_map(|version| {
                version["intelligence_presets"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .cloned()
            })
            .collect();
        for model in value["models"].as_array().into_iter().flatten() {
            presets.push(json!({"model_slug":model["slug"],"title":model["title"].as_str().unwrap_or_else(||model["slug"].as_str().unwrap())}));
        }
        let version = json!({"id":"service-model-catalog","display_text":"可用模型","slugs":allowed,"intelligence_presets":presets});
        if let Some(versions) = value["versions"].as_array_mut() {
            versions.insert(0, version);
        } else {
            value["versions"] = json!([version]);
        }
    }
    super::model_tiers::desktop_options(value, &ultrafast);
    if let Some(policy) = value.get_mut("workspace_model_policy")
        && !policy["selection"]["model"].is_null()
        && !permits(&policy["selection"]["model"])
    {
        policy["selection"]["model"] = Value::Null;
        policy["selection"]["thinking_effort"] = Value::Null;
    }
    Ok(())
}

pub(crate) async fn codex_model_catalog(storage: &Storage, owner: &str) -> crate::Result<Value> {
    let ultrafast = super::model_tiers::ultrafast_models(storage).await?;
    let models = storage
        .available_virtual_models(owner, codex2api_core::CHATGPT)
        .await?;
    let descriptors = models
        .iter()
        .filter(|m| m.kind == "text")
        .map(|model| super::model_tiers::codex_descriptor(&model.model, &ultrafast))
        .collect::<Vec<_>>();
    Ok(json!({"models":descriptors}))
}

/// Replace identity fields and exact identifiers in non-streaming backend metadata only.
pub fn mask(
    value: &mut Value,
    account: &PlatformAccount,
    real: &codex2api_storage::SupplierAccount,
) {
    match value {
        Value::String(s) => {
            if real.chatgpt_account_id.as_deref() == Some(s.as_str()) {
                *s = account.id.clone();
            } else if real.chatgpt_user_id.as_deref() == Some(s.as_str()) {
                *s = format!("user-{}", account.id);
            } else if real.email.as_deref() == Some(s.as_str()) {
                *s = account.email.clone();
            }
        }
        Value::Array(values) => {
            for item in values {
                mask(item, account, real);
            }
        }
        Value::Object(object) => {
            object.retain(|key, _| {
                !matches!(
                    key.to_ascii_lowercase().as_str(),
                    "supplier_account_id"
                        | "supplier_id"
                        | "installation_id"
                        | "access_token"
                        | "refresh_token"
                        | "id_token"
                        | "authorization"
                        | "api_key"
                        | "cookie"
                        | "cookies"
                )
            });
            let identity_object = object.contains_key("email")
                || object.contains_key("user_id")
                || object.contains_key("chatgpt_user_id")
                || object.get("id").and_then(Value::as_str).is_some_and(|id| {
                    real.chatgpt_account_id.as_deref() == Some(id)
                        || real.chatgpt_user_id.as_deref() == Some(id)
                });
            for (key, item) in object.iter_mut() {
                match key.as_str() {
                    "error" if !item.is_null() => *item = crate::public_output::error(item),
                    "plan_type" | "chatgpt_plan_type" => *item = account.plan_type.as_str().into(),
                    "email" => *item = account.email.clone().into(),
                    "account_id" | "chatgpt_account_id" => *item = account.id.clone().into(),
                    "user_id" | "chatgpt_user_id" => *item = format!("user-{}", account.id).into(),
                    "name" | "display_name" if identity_object => {
                        *item = account.name.clone().into()
                    }
                    _ => mask(item, account, real),
                }
            }
            if let Some(real_id) = real.chatgpt_account_id.as_ref()
                && let Some(item) = object.remove(real_id)
            {
                object.insert(account.id.clone(), item);
            }
        }
        _ => {}
    }
}
pub async fn quota_headers(
    storage: &Storage,
    id: &str,
    headers: &mut HeaderMap,
) -> crate::Result<()> {
    let quota = storage.execution_quota(id).await?;
    let names: Vec<_> = headers
        .keys()
        .filter(|k| {
            k.as_str() == "x-codex-plan-type"
                || k.as_str().starts_with("x-codex-")
                    && (k.as_str().contains("primary")
                        || k.as_str().contains("secondary")
                        || k.as_str().contains("credits")
                        || k.as_str().contains("limit"))
        })
        .cloned()
        .collect();
    for name in names {
        headers.remove(name);
    }
    for (name, value) in [
        ("x-codex-credits-has-credits", "false"),
        ("x-codex-credits-unlimited", "false"),
    ] {
        headers.insert(name, value.parse().unwrap());
    }
    headers.insert(
        "x-codex-plan-type",
        quota["plan_type"]
            .as_str()
            .ok_or_else(|| crate::ApiError::internal("Missing subscription plan."))?
            .parse()
            .map_err(|_| crate::ApiError::internal("Invalid subscription plan."))?,
    );
    for (side, window) in [
        ("primary", "primary_window"),
        ("secondary", "secondary_window"),
    ] {
        let value = &quota["rate_limit"][window];
        if value.is_null() {
            continue;
        }
        for (suffix, text) in [
            ("used-percent", value["used_percent"].to_string()),
            ("reset-at", value["reset_at"].to_string()),
            (
                "reset-after-seconds",
                value["reset_after_seconds"].to_string(),
            ),
            (
                "window-minutes",
                (value["limit_window_seconds"].as_i64().unwrap() / 60).to_string(),
            ),
        ] {
            headers.insert(
                format!("x-codex-{side}-{suffix}")
                    .parse::<axum::http::HeaderName>()
                    .unwrap(),
                text.parse().unwrap(),
            );
        }
    }
    Ok(())
}

/// Remove supplier quota events from SSE as well as the WebSocket path. Frames
/// may span arbitrary transport chunks; unrelated frames remain byte-for-byte.
pub fn isolate_sse(body: axum::body::Body, storage: Storage, id: String) -> axum::body::Body {
    use axum::body::Bytes;
    use futures::StreamExt;
    let stream = futures::stream::try_unfold(
        (
            body.into_data_stream(),
            bytes::BytesMut::new(),
            0usize,
            false,
            storage,
            id,
        ),
        |(mut source, mut buffer, mut scanned, mut ended, storage, id)| async move {
            loop {
                let mut boundary = None;
                while scanned < buffer.len() {
                    scanned += 1;
                    if (scanned >= 2 && &buffer[scanned - 2..scanned] == b"\n\n")
                        || (scanned >= 4 && &buffer[scanned - 4..scanned] == b"\r\n\r\n")
                    {
                        boundary = Some(scanned);
                        break;
                    }
                }
                if let Some(end) =
                    boundary.or_else(|| (ended && !buffer.is_empty()).then_some(buffer.len()))
                {
                    let frame = buffer.split_to(end).freeze();
                    scanned = 0;
                    let mut output = frame.clone();
                    if let Ok(text) = std::str::from_utf8(&frame) {
                        let data = codex2api_upstream::sse_data(text);
                        let event = serde_json::from_str::<Value>(&data);
                        if let Ok(mut value) = event {
                            if value["type"] == "codex.rate_limits" {
                                let usage = storage
                                    .execution_quota(&id)
                                    .await
                                    .map_err(std::io::Error::other)?;
                                let window = |key: &str| {
                                    let v = &usage["rate_limit"][key];
                                    if v.is_null() {
                                        Value::Null
                                    } else {
                                        json!({"used_percent":v["used_percent"],"window_minutes":v["limit_window_seconds"].as_i64().unwrap()/60,"reset_at":v["reset_at"]})
                                    }
                                };
                                let value = json!({"type":"codex.rate_limits","plan_type":usage["plan_type"],"credits":usage["credits"],"rate_limits":{"primary":window("primary_window"),"secondary":window("secondary_window")}});
                                let mut lines = text
                                    .lines()
                                    .filter(|line| !line.starts_with("data:") && !line.is_empty())
                                    .map(str::to_owned)
                                    .collect::<Vec<_>>();
                                lines.push(format!("data: {value}"));
                                output = Bytes::from(format!("{}\n\n", lines.join("\n")));
                            } else if crate::public_output::metadata(&mut value) {
                                let event_name = text.lines().find(|l| l.starts_with("event:"));
                                output = Bytes::from(format!(
                                    "{}data: {value}\n\n",
                                    event_name
                                        .map(|name| format!("{name}\n"))
                                        .unwrap_or_default()
                                ));
                            }
                        } else if !data.is_empty() && data != "[DONE]" {
                            return Err(std::io::Error::other("Invalid service event"));
                        }
                    } else {
                        return Err(std::io::Error::other("Invalid service event encoding"));
                    }
                    return Ok::<_, std::io::Error>(Some((
                        output,
                        (source, buffer, scanned, ended, storage, id),
                    )));
                }
                if ended {
                    return Ok(None);
                }
                match source.next().await {
                    Some(Ok(bytes)) => buffer.extend_from_slice(&bytes),
                    Some(Err(e)) => return Err(std::io::Error::other(e)),
                    None => ended = true,
                }
                if buffer.len() > codex2api_upstream::MAX_REQUEST_BYTES {
                    return Err(std::io::Error::other("Oversized upstream SSE event"));
                }
            }
        },
    );
    axum::body::Body::from_stream(stream)
}

#[cfg(test)]
#[tokio::test]
#[ignore = "manual fragmented stream performance comparison"]
async fn benchmark_fragmented_sse() {
    use axum::body::{Body, Bytes};
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("stream-pg"))
        .await
        .unwrap();
    let frame = Bytes::from(format!(
        "data: {}\n\n",
        json!({"type":"response.output_text.delta","delta":"x".repeat(128 * 1024)})
    ));
    let start = std::time::Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..3 {
        let (frame, storage) = (frame.clone(), storage.clone());
        tasks.spawn(async move {
            for _ in 0..10 {
                let chunks: Vec<_> = (0..frame.len())
                    .step_by(1024)
                    .map(|at| {
                        Ok::<_, std::io::Error>(frame.slice(at..(at + 1024).min(frame.len())))
                    })
                    .collect();
                let body = isolate_sse(
                    Body::from_stream(futures::stream::iter(chunks)),
                    storage.clone(),
                    "owner".into(),
                );
                assert_eq!(axum::body::to_bytes(body, usize::MAX).await.unwrap(), frame);
            }
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    eprintln!(
        "fragmented SSE: 3 streams x 10 events x 128KiB = {:?}",
        start.elapsed()
    );
}

#[cfg(test)]
#[tokio::test]
async fn fragmented_events_preserve_bytes_and_sanitize_metadata_across_mixed_boundaries() {
    use axum::body::{Body, Bytes};
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("frames-pg"))
        .await
        .unwrap();
    let prefix = ": keepalive\r\n\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"分片🙂\"}\r\n\r\n";
    let text = format!(
        "{prefix}event: response.failed\ndata: {{\"type\":\"response.failed\",\"response\":{{\"error\":{{\"code\":\"slow_down\",\"message\":\"supplier-secret\",\"headers\":{{\"Retry-After\":7}}}},\"metadata\":{{\"ACCOUNT_ID\":\"private\"}}}}}}\n\ndata: [DONE]\n\n"
    );
    for size in [1, 2, 3, 17, text.len()] {
        let chunks = text
            .as_bytes()
            .chunks(size)
            .map(|chunk| Ok::<_, std::io::Error>(Bytes::copy_from_slice(chunk)))
            .collect::<Vec<_>>();
        let body = isolate_sse(
            Body::from_stream(futures::stream::iter(chunks)),
            storage.clone(),
            "owner".into(),
        );
        let output = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let output = std::str::from_utf8(&output).unwrap();
        assert!(output.starts_with(prefix));
        assert!(output.ends_with("data: [DONE]\n\n"));
        assert!(!output.contains("supplier-secret") && !output.contains("private"));
        let error: Value = serde_json::from_str(
            codex2api_upstream::sse_data(
                output
                    .strip_prefix(prefix)
                    .unwrap()
                    .split("\n\n")
                    .next()
                    .unwrap(),
            )
            .as_ref(),
        )
        .unwrap();
        assert_eq!(error["response"]["error"]["headers"]["retry-after"], "7");
    }
}

/// The pinned client reads quota both from handshake headers and codex.rate_limits events.
#[cfg(test)]
pub async fn websocket_message(storage: &Storage, hash: &str, text: &str) -> crate::Result<String> {
    let mut event = serde_json::from_str::<Value>(text)
        .map_err(|_| crate::ApiError::internal("Invalid service event"))?;
    websocket_event(storage, hash, text, &mut event).await
}

pub(crate) async fn websocket_event(
    storage: &Storage,
    hash: &str,
    text: &str,
    event: &mut Value,
) -> crate::Result<String> {
    if event["type"] != "codex.rate_limits" {
        return Ok(if crate::public_output::metadata(event) {
            event.to_string()
        } else {
            text.to_owned()
        });
    }
    let access = storage
        .virtual_access(hash)
        .await?
        .ok_or_else(crate::ApiError::invalid_token)?;
    let usage = storage.execution_quota(&access.virtual_account_id).await?;
    let window = |name: &str| {
        let v = &usage["rate_limit"][name];
        if v.is_null() {
            return Value::Null;
        }
        json!({"used_percent":v["used_percent"],"window_minutes":v["limit_window_seconds"].as_i64().unwrap()/60,"reset_at":v["reset_at"]})
    };
    *event = json!({"type":"codex.rate_limits","rate_limits":{"primary":window("primary_window"),"secondary":window("secondary_window")},"credits":usage["credits"]});
    event["plan_type"] = usage["plan_type"].clone();
    Ok(event.to_string())
}

#[cfg(test)]
mod isolation_tests {
    use super::*;

    #[tokio::test]
    async fn responses_events_keep_partial_answers_and_nested_retry_advice() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("events-pg"))
            .await
            .unwrap();
        let events = [
            json!({"type":"response.output_item.done","item":{"id":"msg_partial","type":"message","role":"assistant","phase":"partial_answer","content":[{"type":"output_text","text":"Partial fixture"}]}}),
            json!({"type":"response.completed","response":{"id":"resp_partial","end_turn":false}}),
            json!({"type":"response.failed","response":{"error":{"code":"server_is_overloaded","message":"supplier-secret","headers":{"Retry-After":"Wed, 21 Oct 2026 07:28:00 GMT","X-Account":"supplier-secret"}}}}),
        ];
        let input = events
            .iter()
            .map(|event| format!("data: {event}\n\n"))
            .collect::<String>();
        let chunks = input
            .as_bytes()
            .chunks(7)
            .map(|chunk| Ok::<_, std::io::Error>(axum::body::Bytes::copy_from_slice(chunk)))
            .collect::<Vec<_>>();
        let output = axum::body::to_bytes(
            isolate_sse(
                axum::body::Body::from_stream(futures::stream::iter(chunks)),
                storage.clone(),
                "fixture".into(),
            ),
            64 * 1024,
        )
        .await
        .unwrap();
        let text = std::str::from_utf8(&output).unwrap();
        let actual = text
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual[0], events[0]);
        assert_eq!(actual[1], events[1]);
        assert_eq!(
            actual[2]["response"]["error"]["headers"],
            json!({"retry-after":"Wed, 21 Oct 2026 07:28:00 GMT"})
        );
        assert!(!text.contains("supplier-secret"));
        for (event, expected) in events.iter().zip(actual) {
            let websocket = websocket_message(&storage, "fixture", &event.to_string())
                .await
                .unwrap();
            assert_eq!(serde_json::from_str::<Value>(&websocket).unwrap(), expected);
        }
    }
    #[tokio::test]
    async fn sse_quota_is_replaced_across_split_frames_without_changing_model_output() {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("sse-pg"))
            .await
            .unwrap();
        let account = PlatformAccount {
            provider_id: "chatgpt".into(),
            id: "virtual".into(),
            username: "virtual".into(),
            password_hash: "unused".into(),
            name: "Virtual".into(),
            email: "v@example.test".into(),
            plan_type: "plus".into(),
            plan_id: "plus".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        storage.save_account_fixture(&account).await.unwrap();
        // SSE 中只有已开始使用的内层窗口才会作为 primary 返回。
        storage
            .insert_usage(&codex2api_storage::UsageRecord {
                id: "prior-request".into(),
                account_id: "supplier".into(),
                subject_id: account.id.clone(),
                endpoint: "/v1/responses".into(),
                transport: "sse".into(),
                requested_at_ms: chrono::Utc::now().timestamp_millis(),
                status: "in_progress".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let mut plan = storage
            .virtual_plan(&account.plan_id)
            .await
            .unwrap()
            .unwrap();
        plan.config["spending_windows"] = serde_json::json!([{"duration_seconds":604800,"cost_limit_usd":"10"},{"duration_seconds":18000,"cost_limit_usd":"0"}]);
        assert!(
            storage
                .save_virtual_plan(&plan, Some(plan.revision))
                .await
                .unwrap()
        );
        let first = "event: response.output_text.delta\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\r\n\r\n";
        let upstream = format!(
            "{first}event: codex.rate_limits\ndata: {{\"type\":\"codex.rate_limits\",\"plan_type\":\"enterprise\",\"rate_limits\":{{\"primary\":{{\"used_percent\":99}}}},\"credits\":{{\"balance\":\"supplier-balance\"}}}}\n\ndata: [DONE]\n\n"
        );
        let chunks: Vec<_> = upstream
            .as_bytes()
            .chunks(3)
            .map(|v| Ok::<_, std::io::Error>(axum::body::Bytes::copy_from_slice(v)))
            .collect();
        let body = isolate_sse(
            axum::body::Body::from_stream(futures::stream::iter(chunks)),
            storage,
            account.id,
        );
        let output = String::from_utf8(
            axum::body::to_bytes(body, 64 * 1024)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(output.starts_with(first));
        assert!(output.ends_with("data: [DONE]\n\n"));
        assert!(!output.contains("supplier-balance"));
        assert!(!output.contains("enterprise"));
        let quota: Value = serde_json::from_str(
            output
                .lines()
                .find_map(|line| {
                    line.strip_prefix("data: ")
                        .filter(|s| s.contains("\"type\":\"codex.rate_limits\""))
                })
                .unwrap(),
        )
        .unwrap();
        assert_eq!(quota["plan_type"], "plus");
        assert_eq!(quota["rate_limits"]["primary"]["used_percent"], 100);
        assert_eq!(quota["rate_limits"]["primary"]["window_minutes"], 300);
        assert_eq!(quota["rate_limits"]["secondary"]["window_minutes"], 10080);
        assert!(
            quota["rate_limits"]["primary"]["reset_at"]
                .as_i64()
                .unwrap()
                > chrono::Utc::now().timestamp()
        );
    }
}

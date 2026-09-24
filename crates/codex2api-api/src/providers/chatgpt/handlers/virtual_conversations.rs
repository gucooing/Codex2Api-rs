use axum::{
    body::{Body, Bytes},
    extract::{Extension, State},
    http::HeaderMap,
    response::Response,
};
use codex2api_storage::VirtualAccess;
use futures::StreamExt;
use serde_json::{Value, json};

pub(crate) async fn forward(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Extension(path): Extension<&'static str>,
    mut headers: HeaderMap,
    body: Bytes,
) -> crate::Result<Response> {
    let started = std::time::Instant::now();
    let requested_at = chrono::Utc::now().timestamp_millis();
    let mut value = codex2api_upstream::decode_body(&body, &headers)?;
    if !value.is_object() {
        return Err(crate::ApiError::bad_request(
            "Expected conversation object.",
        ));
    }
    for key in ["account_id", "conversation_owner_id"] {
        if let Some(v) = value.get(key).filter(|v| !v.is_null()) {
            super::virtual_data::account_match(
                &access,
                v.as_str()
                    .ok_or_else(|| crate::ApiError::bad_request("Invalid owner."))?,
            )?;
        }
    }
    let (credential, ctx) = crate::providers::chatgpt::access::resolve_supplier(
        &state,
        &headers,
        Extension(access.clone()),
    )
    .await?;
    if let Some(id) = value["conversation_id"].as_str() {
        let (source, _) = state
            .storage
            .virtual_resource(&access.virtual_account_id, "conversation", id)
            .await?
            .ok_or_else(super::virtual_data::not_found)?;
        if source.as_deref() != Some(ctx.account.id.as_str()) {
            return Err(super::virtual_data::not_found());
        }
    } else if matches!(
        path,
        "/backend-api/f/conversation/resume" | "/backend-api/stop_conversation"
    ) {
        return Err(crate::ApiError::bad_request(
            "A conversation ID is required.",
        ));
    }
    let execution = crate::execution::ExecutionContext::new(
        state.storage.clone(),
        &ctx.account,
        &credential.id,
        &credential.name,
        path,
        "http",
    );
    if path == "/backend-api/f/conversation/prepare" || value.get("model").is_some() {
        execution
            .authorize(
                &codex2api_upstream::request_metadata(&body, &headers)?,
                true,
            )
            .await?;
    }
    if let Some(local) = headers.get("x-conduit-token").and_then(|h| h.to_str().ok()) {
        let upstream = state
            .storage
            .virtual_conduit(&access.virtual_account_id, &ctx.account.id, local)
            .await?
            .ok_or_else(crate::ApiError::invalid_token)?;
        headers.insert(
            "x-conduit-token",
            upstream
                .parse()
                .map_err(|_| crate::ApiError::internal("Invalid conduit token."))?,
        );
    }
    for key in ["account_id", "conversation_owner_id"] {
        if value.get(key).is_some() {
            value[key] = ctx.account.chatgpt_account_id.clone().into();
        }
    }
    headers.remove("content-encoding");
    headers.remove("content-length");
    let body = Bytes::from(value.to_string());
    let mut log = if path == "/backend-api/f/conversation" {
        crate::providers::chatgpt::access::check_virtual_quota(
            &state.storage,
            &access.virtual_account_id,
        )
        .await?;
        Some(
            execution
                .start(
                    codex2api_upstream::request_metadata(&body, &headers)?,
                    started,
                    requested_at,
                )
                .await?,
        )
    } else {
        None
    };
    let client = state.upstream.get(&ctx.account.id).await?;
    let response = match client
        .forward_chatgpt_conversation(path, body, headers)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            if let Some(log) = &mut log {
                log.upstream_failure(&e);
            }
            return Err(e.into());
        }
    };
    let status = response.status();
    if path == "/backend-api/f/conversation/prepare" && status.is_success() {
        let mut value: Value = response
            .json()
            .await
            .map_err(|_| crate::ApiError::internal("Invalid conversation preparation response."))?;
        if !value.is_object() {
            return Err(crate::ApiError::internal(
                "Invalid conversation preparation object.",
            ));
        }
        let account = state
            .storage
            .virtual_account(&access.virtual_account_id)
            .await?
            .ok_or_else(crate::ApiError::invalid_token)?;
        crate::providers::chatgpt::identity::mask(&mut value, &account, &ctx.account);
        if value.get("rate_limit").is_some() {
            value["rate_limit"] = state
                .storage
                .virtual_quota(&access.virtual_account_id)
                .await?["rate_limit"]
                .clone();
        }
        if let Some(token) = value["conduit_token"].as_str() {
            value["conduit_token"] = state
                .storage
                .create_virtual_conduit(&access.virtual_account_id, &ctx.account.id, token)
                .await?
                .into();
        }
        // Save only preparation metadata. Supplier conduit credentials are kept in
        // a separate table and are never shown on the account management page.
        let id = uuid::Uuid::new_v4().to_string();
        state.storage.save_virtual_resource(&access.virtual_account_id,"conversation_preparation",&id,Some(&ctx.account.id),&json!({"id":id,"prepared_at":chrono::Utc::now().to_rfc3339(),"conversation_id":value["conversation_id"],"has_conduit":value["conduit_token"].is_string()})).await?;
        return Ok(crate::providers::chatgpt::identity::json_response(value));
    }
    let mut headers = response.headers().clone();
    crate::providers::chatgpt::identity::quota_headers(
        &state.storage,
        &access.virtual_account_id,
        &mut headers,
    )
    .await?;
    if let Some(token) = headers.get("x-conduit-token").and_then(|h| h.to_str().ok()) {
        let token = state
            .storage
            .create_virtual_conduit(&access.virtual_account_id, &ctx.account.id, token)
            .await?;
        headers.insert("x-conduit-token", token.parse().unwrap());
    }
    let sse = headers
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"));
    let capture = |body| {
        if sse && status.is_success() {
            record_conversations(
                body,
                state.storage.clone(),
                access.virtual_account_id.clone(),
                ctx.account.id,
            )
        } else {
            body
        }
    };
    let body = if let Some(mut log) = log {
        log.http_status(status.as_u16());
        log.wrap_with(response, capture)
    } else {
        capture(Body::from_stream(response.bytes_stream()))
    };
    Ok(crate::response::forward_response(status, headers, body))
}

fn record_conversations(
    body: Body,
    storage: codex2api_storage::Storage,
    owner: String,
    source: String,
) -> Body {
    let stream = futures::stream::try_unfold(
        (
            body.into_data_stream(),
            Vec::new(),
            false,
            storage,
            owner,
            source,
        ),
        |(mut input, mut buffer, mut ended, storage, owner, source)| async move {
            loop {
                let end = buffer
                    .windows(2)
                    .position(|w| w == b"\n\n")
                    .map(|i| i + 2)
                    .or_else(|| {
                        buffer
                            .windows(4)
                            .position(|w| w == b"\r\n\r\n")
                            .map(|i| i + 4)
                    });
                if let Some(end) =
                    end.or_else(|| (ended && !buffer.is_empty()).then_some(buffer.len()))
                {
                    let frame: Vec<u8> = buffer.drain(..end).collect();
                    let mut output = frame.clone();
                    if let Ok(text) = std::str::from_utf8(&frame) {
                        let data = text
                            .lines()
                            .filter_map(|l| l.strip_prefix("data:").map(str::trim_start))
                            .collect::<Vec<_>>()
                            .join("\n");
                        if let Ok(value) = serde_json::from_str::<Value>(&data) {
                            if value["type"] == "conversation_detail_metadata" {
                                let mut local = storage
                                    .virtual_config(&owner, "conversation_metadata")
                                    .await
                                    .map_err(std::io::Error::other)?
                                    .value;
                                if value.get("conversation_id").is_some() {
                                    local["conversation_id"] = value["conversation_id"].clone();
                                }
                                let mut lines = text
                                    .lines()
                                    .filter(|line| !line.starts_with("data:") && !line.is_empty())
                                    .map(str::to_owned)
                                    .collect::<Vec<_>>();
                                lines.push(format!("data: {local}"));
                                output = format!("{}\n\n", lines.join("\n")).into_bytes();
                            }
                            if let Some(id) = value["conversation_id"]
                                .as_str()
                                .filter(|s| !s.is_empty() && s.len() <= 128)
                            {
                                let mut summary=storage.virtual_resource(&owner,"conversation",id).await.map_err(std::io::Error::other)?.map(|(_,v)|v).unwrap_or_else(||json!({"id":id,"conversation_id":id,"title":id,"create_time":chrono::Utc::now().timestamp(),"is_archived":false}));
                                summary["update_time"] = chrono::Utc::now().timestamp().into();
                                if let Some(status) = value["message"]["status"].as_str() {
                                    summary["status"] = status.into();
                                }
                                storage
                                    .save_virtual_resource(
                                        &owner,
                                        "conversation",
                                        id,
                                        Some(&source),
                                        &summary,
                                    )
                                    .await
                                    .map_err(std::io::Error::other)?;
                            }
                        }
                    }
                    return Ok::<_, std::io::Error>(Some((
                        Bytes::from(output),
                        (input, buffer, ended, storage, owner, source),
                    )));
                }
                if ended {
                    return Ok(None);
                }
                match input.next().await {
                    Some(Ok(bytes)) => buffer.extend_from_slice(&bytes),
                    Some(Err(e)) => return Err(std::io::Error::other(e)),
                    None => ended = true,
                }
                if buffer.len() > codex2api_upstream::MAX_REQUEST_BYTES {
                    return Err(std::io::Error::other("Oversized conversation event"));
                }
            }
        },
    );
    Body::from_stream(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn native_stream_records_real_conversation_ownership_and_keeps_metadata_local() {
        let dir = tempfile::tempdir().unwrap();
        let storage = codex2api_storage::Storage::open(dir.path().join("conversation.sqlite"))
            .await
            .unwrap();
        let source = codex2api_accounts::SupplierAccountStore::open(storage.clone())
            .create_pending()
            .await
            .unwrap()
            .account;
        for id in ["one", "two"] {
            storage
                .save_virtual_account(&codex2api_storage::VirtualAccount {
                    provider_id: "chatgpt".into(),
                    id: id.into(),
                    username: id.into(),
                    password_hash: "unused".into(),
                    name: id.into(),
                    email: format!("{id}@example.test"),
                    plan_type: "plus".into(),
                    plan_id: "plus".into(),
                    subscription_expires_at: None,
                    enabled: true,
                    created_at: chrono::Utc::now().to_rfc3339(),
                })
                .await
                .unwrap();
        }
        let content = "data: {\"conversation_id\":\"actual-upstream-id\",\"message\":{\"status\":\"finished_successfully\",\"content\":{\"parts\":[\"preserved text\"]}}}\n\n";
        let original = format!(
            "data: {{\"type\":\"conversation_detail_metadata\",\"conversation_id\":\"actual-upstream-id\",\"blocked_features\":[\"supplier-private-limit\"]}}\n\n{content}data: [DONE]\n\n"
        );
        let chunks: Vec<_> = original
            .as_bytes()
            .chunks(5)
            .map(|c| Ok::<_, std::io::Error>(Bytes::copy_from_slice(c)))
            .collect();
        let body = record_conversations(
            Body::from_stream(futures::stream::iter(chunks)),
            storage.clone(),
            "one".into(),
            source.id.clone(),
        );
        let bytes = axum::body::to_bytes(body, 64 * 1024).await.unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains(content));
        assert!(!text.contains("supplier-private-limit"));
        let (bound, record) = storage
            .virtual_resource("one", "conversation", "actual-upstream-id")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bound.as_deref(), Some(source.id.as_str()));
        assert_eq!(record["status"], "finished_successfully");
        assert!(!record.to_string().contains("preserved text"));
        let events = storage.virtual_events("one", 0).await.unwrap();
        assert!(
            events
                .iter()
                .any(|e| e["payload"]["type"] == "conversation-created")
        );
        assert!(
            events
                .iter()
                .any(|e| e["payload"]["type"] == "conversation-turn-complete")
        );
        assert!(
            storage
                .virtual_events("two", 0)
                .await
                .unwrap()
                .iter()
                .all(|e| e["topic"] != "conversations")
        );
        assert!(
            storage
                .virtual_resource("two", "conversation", "actual-upstream-id")
                .await
                .unwrap()
                .is_none()
        );
    }
}

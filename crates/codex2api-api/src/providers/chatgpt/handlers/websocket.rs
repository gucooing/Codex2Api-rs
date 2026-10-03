use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::extract::{Extension, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as UpstreamMessage;

use crate::providers::chatgpt::access::AccessCheck;
use crate::{ApiState, Result};
use codex2api_upstream::{Endpoint, UpstreamWebSocket, normalize_response_identity};

pub(crate) struct ResponseSession {
    workspace: Option<codex2api_upstream::WorkspaceConnection>,
    guardian_reviewer: bool,
}

pub async fn responses_websocket(
    _: crate::user_agent::AllowedUserAgent,
    State(state): State<ApiState>,
    Extension(endpoint): Extension<Endpoint>,
    oauth: Extension<codex2api_storage::VirtualAccess>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    super::pooled_websocket::connect(state, endpoint, oauth.0, headers, upgrade).await
}

pub(super) fn prepare_message(
    text: &str,
    installation_id: &str,
    timezone: Option<&str>,
    guardian_reviewer: bool,
) -> codex2api_upstream::Result<String> {
    let mut value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| codex2api_upstream::UpstreamError::InvalidRequest(e.to_string()))?;
    if value.get("type").and_then(serde_json::Value::as_str) == Some("response.create") {
        codex2api_upstream::apply_response_timezone(&mut value, timezone)?;
        let mut headers = HeaderMap::new();
        if guardian_reviewer {
            headers.insert(
                "x-codex-guardian",
                axum::http::HeaderValue::from_static("reviewer"),
            );
        }
        normalize_response_identity(&mut value, installation_id, &headers)?;
    }
    Ok(serde_json::to_string(&value)?)
}

pub(crate) fn prepare_realtime_message(
    text: &str,
    installation_id: &str,
) -> codex2api_upstream::Result<String> {
    let mut value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| codex2api_upstream::UpstreamError::InvalidRequest(e.to_string()))?;
    let mut changed = false;
    if value.get("client_metadata").is_some() {
        normalize_response_identity(&mut value, installation_id, &HeaderMap::new())?;
        changed = true;
    }
    if let Some(session) = value.get_mut("session")
        && session.get("client_metadata").is_some()
    {
        normalize_response_identity(session, installation_id, &HeaderMap::new())?;
        changed = true;
    }
    if changed {
        Ok(serde_json::to_string(&value)?)
    } else {
        Ok(text.to_string())
    }
}

pub(crate) async fn bridge_recorded(
    client: WebSocket,
    upstream: UpstreamWebSocket,
    installation_id: String,
    realtime: bool,
    timezone: Option<String>,
    ledger: crate::usage::WsLedger,
    consumer_access: (codex2api_storage::Storage, AccessCheck),
    response_session: Option<ResponseSession>,
) {
    let connected_at = std::time::Instant::now();
    let ledger = tokio::sync::Mutex::new(ledger);
    let (storage, access) = consumer_access;
    let (mut client_tx, mut client_rx) = client.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();
    let guardian_reviewer = response_session
        .as_ref()
        .is_some_and(|session| session.guardian_reviewer);
    let to_upstream = async {
        while let Some(message) = client_rx.next().await {
            let message = match message {
                Ok(message) => message,
                Err(error) => {
                    ledger.lock().await.client_stopped().await?;
                    return Err(relay_error(error));
                }
            };
            if matches!(message, Message::Text(_) | Message::Binary(_))
                && let Some(connection) = response_session
                    .as_ref()
                    .and_then(|session| session.workspace.as_ref())
            {
                connection.check_current().await?;
            }
            if matches!(&message, Message::Text(_) | Message::Binary(_)) {
                access.validate(&storage).await?;
            }
            let message = match message {
                Message::Text(text) => UpstreamMessage::Text(
                    (if realtime {
                        prepare_realtime_message(&text, &installation_id)
                    } else {
                        prepare_message(
                            &text,
                            &installation_id,
                            timezone.as_deref(),
                            guardian_reviewer,
                        )
                    })?
                    .into(),
                ),
                Message::Binary(bytes) if realtime => UpstreamMessage::Binary(bytes),
                Message::Binary(bytes) => {
                    let text = std::str::from_utf8(&bytes).map_err(|_| {
                        crate::ApiError::bad_request("Expected a UTF-8 Responses frame.")
                    })?;
                    UpstreamMessage::Text(
                        prepare_message(
                            text,
                            &installation_id,
                            timezone.as_deref(),
                            guardian_reviewer,
                        )?
                        .into(),
                    )
                }
                Message::Close(frame) => {
                    ledger.lock().await.client_stopped().await?;
                    upstream_tx
                        .send(UpstreamMessage::Close(frame.map(|f| {
                            tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                code: f.code.into(),
                                reason: f.reason.to_string().into(),
                            }
                        })))
                        .await
                        .map_err(relay_error)?;
                    return Ok::<_, crate::ApiError>(());
                }
                Message::Ping(_) | Message::Pong(_) => continue,
            };
            if realtime {
                let text = if let UpstreamMessage::Text(text) = &message {
                    Some(text.as_str())
                } else {
                    None
                };
                crate::usage::realtime_start(&ledger, text).await?;
            } else if let UpstreamMessage::Text(text) = &message {
                crate::usage::ws_start(&ledger, text.as_str()).await?;
            }
            if let Err(error) = upstream_tx.send(message).await {
                let message = upstream_failure_message("发送", &error);
                tracing::warn!(account_id=%access.account_id, reason=%message, "upstream WebSocket write failed");
                ledger
                    .lock()
                    .await
                    .fail_inflight("upstream_websocket_write_error", &message)
                    .await?;
                // A broken established connection does not establish an account outage.
                return Err(relay_error(message));
            }
        }
        ledger.lock().await.client_stopped().await?;
        let _ = upstream_tx.close().await;
        Ok(())
    };
    let to_client = async {
        let mut last_message_at = connected_at;
        while let Some(message) = upstream_rx.next().await {
            let received_at = std::time::Instant::now();
            let message = match message {
                Ok(message) => message,
                Err(error) => {
                    let message = upstream_failure_message("响应读取", &error);
                    tracing::warn!(
                        account_id=%access.account_id,
                        reason=%message,
                        connection_ms=connected_at.elapsed().as_millis() as u64,
                        since_last_message_ms=last_message_at.elapsed().as_millis() as u64,
                        "upstream WebSocket read failed"
                    );
                    ledger
                        .lock()
                        .await
                        .fail_inflight("upstream_websocket_read_error", &message)
                        .await?;
                    return Err(relay_error(message));
                }
            };
            last_message_at = received_at;
            let access_result = access.validate(&storage).await;
            let allowed = access_result.is_ok();
            let bytes = match &message {
                UpstreamMessage::Text(text) => Some(text.as_bytes()),
                UpstreamMessage::Binary(bytes) => Some(bytes.as_ref()),
                _ => None,
            };
            let outcome = if let Some(bytes) = bytes {
                ledger
                    .lock()
                    .await
                    .observe_at(bytes, allowed, received_at)
                    .await?
            } else {
                None
            };
            access_result?;
            let message = match message {
                UpstreamMessage::Text(text) => {
                    let text = crate::providers::chatgpt::identity::websocket_message(
                        &storage,
                        &access.hash,
                        &text,
                    )
                    .await?;
                    Message::Text(text.into())
                }
                UpstreamMessage::Binary(bytes) => {
                    if !realtime {
                        let text = std::str::from_utf8(&bytes).map_err(|_| {
                            crate::ApiError::bad_request("Expected a UTF-8 Responses frame.")
                        })?;
                        Message::Binary(
                            crate::providers::chatgpt::identity::websocket_message(
                                &storage,
                                &access.hash,
                                text,
                            )
                            .await?
                            .into_bytes()
                            .into(),
                        )
                    } else {
                        Message::Binary(bytes)
                    }
                }
                UpstreamMessage::Close(frame) => {
                    if ledger.lock().await.has_pending() {
                        return Err(crate::ApiError::openai(
                            axum::http::StatusCode::BAD_GATEWAY,
                            "server_error",
                            "Upstream WebSocket closed before the generation ended.",
                            Some("upstream_websocket_closed"),
                        ));
                    }
                    client_tx
                        .send(Message::Close(frame.map(|f| CloseFrame {
                            code: f.code.into(),
                            reason: f.reason.to_string().into(),
                        })))
                        .await
                        .map_err(relay_error)?;
                    return Ok::<_, crate::ApiError>(());
                }
                UpstreamMessage::Ping(_) | UpstreamMessage::Pong(_) | UpstreamMessage::Frame(_) => {
                    continue;
                }
            };
            if let Err(error) = client_tx.send(message).await {
                ledger.lock().await.client_stopped().await?;
                return Err(relay_error(error));
            }
            if let Some(codex2api_upstream::ResponseOutcome::Failed(failure)) = outcome {
                ledger.lock().await.fail_pending(failure.clone()).await?;
                client_tx
                    .send(Message::Close(Some(CloseFrame {
                        code: if failure.status.is_some_and(|s| s < 500) {
                            1008
                        } else {
                            1011
                        },
                        reason: "Responses generation failed".into(),
                    })))
                    .await
                    .map_err(relay_error)?;
                return Ok(());
            }
        }
        if ledger.lock().await.has_pending() {
            return Err(crate::ApiError::openai(
                axum::http::StatusCode::BAD_GATEWAY,
                "server_error",
                "Upstream WebSocket ended before the generation ended.",
                Some("upstream_websocket_closed"),
            ));
        }
        let _ = client_tx.close().await;
        Ok(())
    };
    let result = tokio::select! { result = to_upstream => result, result = to_client => result };
    if let Err(error) = result {
        tracing::debug!(%error, "Responses WebSocket relay ended");
        let (status, message) = error_message(error).await;
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&message) {
            let code = value["error"]["code"].as_str().unwrap_or("relay_error");
            let reason = value["error"]["message"]
                .as_str()
                .unwrap_or("服务端转发异常，请求未完成");
            if let Err(error) = ledger
                .lock()
                .await
                .fail_pending(codex2api_upstream::ResponseFailure::new(
                    Some(status),
                    Some(code),
                    Some(reason),
                ))
                .await
            {
                tracing::error!(%error, "failed to settle interrupted WebSocket requests");
            }
        }
        // A rejected frame is still an application error. Dropping the TCP
        // connection here hid quota/auth failures behind ResetWithoutClosingHandshake.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            client_tx.send(Message::Text(message.into())).await?;
            client_tx
                .send(Message::Close(Some(CloseFrame {
                    code: if status < 500 { 1008 } else { 1011 },
                    reason: "Responses relay ended".into(),
                })))
                .await
        })
        .await;
    } else {
        // Flush automatic close replies if the peer initiated shutdown.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), client_tx.close()).await;
    }
}

fn upstream_failure_message(
    operation: &str,
    error: &tokio_tungstenite::tungstenite::Error,
) -> String {
    use tokio_tungstenite::tungstenite::{Error, error::ProtocolError};
    // Display preserves transport/protocol causes without dumping frames or HTTP bodies.
    // Omit variants that can contain arbitrary peer text or a credential-bearing URL.
    let detail = match error {
        Error::Io(error) => format!("I/O {:?}: {error}", error.kind()),
        Error::Utf8(_) => "UTF-8 encoding error".into(),
        Error::Url(_) => "WebSocket URL error".into(),
        Error::Protocol(ProtocolError::InvalidExtensionsHeader(_)) => {
            "Invalid WebSocket extensions header".into()
        }
        _ => error.to_string(),
    };
    crate::usage::error_text(&format!("上游 WebSocket {operation}失败：{detail}"), 512)
}

pub(super) fn relay_error(error: impl std::fmt::Display) -> crate::ApiError {
    tracing::debug!(%error, "WebSocket transport or frame error");
    crate::ApiError::openai(
        axum::http::StatusCode::BAD_GATEWAY,
        "server_error",
        "The Responses connection ended before completion.",
        Some("stream_error"),
    )
}

pub(super) async fn error_message(error: crate::ApiError) -> (u16, String) {
    let response = error.into_response();
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap_or_default();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        serde_json::json!({"error":{"type":"server_error","message":"Responses relay failed."}})
    });
    value["type"] = "error".into();
    value["status"] = status.into();
    if let Some(retry_after) = retry_after {
        value["headers"] = serde_json::json!({"retry-after":retry_after});
    }
    (status, value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn received_completion_is_settled_before_revoked_client_is_closed() {
        recorded_connection_fixture("revoked").await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn client_and_upstream_close_frames_have_distinct_results_and_keep_usage() {
        recorded_connection_fixture("client_closed").await;
        recorded_connection_fixture("upstream_closed").await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upstream_read_failure_preserves_supplier_health_and_records_the_cause() {
        recorded_connection_fixture("upstream_read_error").await;
        recorded_connection_fixture("upstream_protocol_error").await;
        recorded_connection_fixture("idle_read_error").await;
        recorded_connection_fixture("read_error_with_supplier_failure").await;
    }

    #[test]
    fn websocket_failure_details_are_bounded_and_do_not_dump_credentials_or_frames() {
        use tokio_tungstenite::tungstenite::{Error, error::UrlError};
        let message = upstream_failure_message(
            "响应读取",
            &Error::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                format!("reset Bearer private-token {}", "x".repeat(1000)),
            )),
        );
        assert!(message.contains("ConnectionReset"));
        assert!(message.contains("[REDACTED]"));
        assert!(!message.contains("private-token"));
        assert_eq!(message.chars().count(), 512);
        for error in [
            Error::WriteBufferFull(Box::new(UpstreamMessage::Text("private-payload".into()))),
            Error::Utf8("private-payload".into()),
            Error::Url(UrlError::UnableToConnect(
                "wss://user:private-payload@example.test/".into(),
            )),
        ] {
            assert!(!upstream_failure_message("发送", &error).contains("private-payload"));
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upstream_ping_is_answered_while_the_client_waits_for_completion() {
        recorded_connection_fixture("upstream_ping").await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compaction_with_timezone_crosses_text_and_binary_websocket_frames() {
        recorded_connection_fixture("compaction_text").await;
        recorded_connection_fixture("compaction_binary").await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upstream_generation_errors_keep_the_event_settle_and_close_without_misclassifying_accounts()
     {
        for ending in [
            "rate_event",
            "quota_event",
            "auth_event",
            "unknown_limit_event",
        ] {
            recorded_connection_fixture(ending).await;
        }
    }

    async fn recorded_connection_fixture(ending: &'static str) {
        use axum::{Router, routing::get};
        use codex2api_storage::{
            OAuthDeviceIdentity, SupplierAccountUpdate, SupplierStatus, VirtualAccount,
        };
        use serde_json::{Value, json};
        let compaction = ending.starts_with("compaction_");
        let error_event = match ending {
            "rate_event" => Some(
                json!({"type":"response.failed","response":{"id":"response-1","error":{"code":"rate_limit_exceeded","message":"retry later"}}}),
            ),
            "quota_event" => Some(
                json!({"type":"response.failed","response":{"id":"response-1","error":{"code":"insufficient_quota","message":"exhausted"}}}),
            ),
            "auth_event" => Some(
                json!({"type":"error","status":401,"error":{"code":"invalid_token","message":"expired"}}),
            ),
            "unknown_limit_event" => {
                Some(json!({"type":"error","status":429,"error":{"message":"limit"}}))
            }
            _ => None,
        };
        let server_error_event = error_event.clone();
        let dir = tempfile::tempdir().unwrap();
        let storage =
            codex2api_storage::Storage::open(dir.path().join("revoked-completion.sqlite"))
                .await
                .unwrap();
        let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
        let supplier = accounts.create_pending().await.unwrap().account;
        storage
            .update_account(
                &supplier.id,
                SupplierAccountUpdate {
                    status: Some(SupplierStatus::Active),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        accounts
            .save_auth_for_account(
                &supplier.id,
                &codex2api_accounts::AuthDotJson::chatgpt(
                    codex2api_accounts::TokenData {
                        id_token: "fixture".into(),
                        access_token: "fixture".into(),
                        refresh_token: "fixture".into(),
                        account_id: Some("fixture".into()),
                    },
                    None,
                ),
            )
            .await
            .unwrap();
        let consumer = VirtualAccount {
            provider_id: "chatgpt".into(),
            id: "consumer".into(),
            username: "consumer".into(),
            password_hash: "fixture".into(),
            name: "Consumer".into(),
            email: "consumer@example.test".into(),
            plan_type: "pro".into(),
            plan_id: "pro".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        storage.save_virtual_account(&consumer).await.unwrap();
        storage
            .save_execution_route(&consumer.id, "chatgpt", Some(&supplier.id), None)
            .await
            .unwrap();
        let device = storage
            .create_virtual_device(&consumer, "refresh", &OAuthDeviceIdentity::default())
            .await
            .unwrap()
            .unwrap();
        storage
            .register_virtual_access(
                &device,
                "refresh",
                "access",
                chrono::Utc::now().timestamp() + 600,
            )
            .await
            .unwrap();
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let supplier_id = supplier.id.clone();
        let read_error = ending.contains("read_error") || ending == "upstream_protocol_error";
        let (complete, ready) = tokio::sync::oneshot::channel();
        let upstream_task = tokio::spawn(async move {
            let (stream, _) = upstream_listener.accept().await.unwrap();
            let mut upstream = tokio_tungstenite::accept_async(stream).await.unwrap();
            let frame = upstream.next().await.unwrap().unwrap();
            assert!(frame.is_text());
            if compaction {
                let request: Value = serde_json::from_slice(&frame.into_data()).unwrap();
                assert_eq!(request["previous_response_id"], "previous");
                assert_eq!(request["input"].as_array().unwrap().len(), 2);
                assert_eq!(request["input"][1]["type"], "compaction_trigger");
                assert!(
                    request["input"][0]
                        .to_string()
                        .contains("<timezone>Asia/Taipei</timezone>")
                );
            }
            upstream.send(UpstreamMessage::Text(json!({"type":"response.created","response":{"id":"response-1","model":"gpt-6-astra","usage":{"input_tokens":10,"output_tokens":2}}}).to_string().into())).await.unwrap();
            ready.await.unwrap();
            if let Some(event) = server_error_event {
                upstream
                    .send(UpstreamMessage::Text(event.to_string().into()))
                    .await
                    .unwrap();
                let _ = upstream.next().await;
                return;
            }
            if ending == "client_closed" {
                assert!(matches!(
                    upstream.next().await.unwrap().unwrap(),
                    UpstreamMessage::Close(_)
                ));
                return;
            }
            if ending == "upstream_closed" {
                upstream.close(None).await.unwrap();
                return;
            }
            if ending == "upstream_protocol_error" {
                use tokio::io::AsyncWriteExt;
                // A frame with an unnegotiated reserved bit is a protocol error,
                // distinct from a network reset, and must remain diagnosable.
                upstream.get_mut().write_all(&[0xc1, 0x00]).await.unwrap();
                return;
            }
            if read_error && ending != "idle_read_error" {
                // Drop TCP without a WebSocket close handshake.
                return;
            }
            if ending == "upstream_ping" {
                upstream
                    .send(UpstreamMessage::Ping(b"heartbeat".to_vec().into()))
                    .await
                    .unwrap();
                assert_eq!(
                    upstream.next().await.unwrap().unwrap(),
                    UpstreamMessage::Pong(b"heartbeat".to_vec().into())
                );
            }
            if compaction {
                upstream.send(UpstreamMessage::Text(json!({"type":"response.output_item.done","response_id":"response-1","output_index":0,"item":{"type":"compaction","encrypted_content":"fixture-compacted-history"}}).to_string().into())).await.unwrap();
            }
            upstream.send(UpstreamMessage::Text(json!({"type":"response.completed","response":{"id":"response-1","model":"gpt-6-astra","status":"completed","usage":{"input_tokens":10,"output_tokens":2,"total_tokens":12}}}).to_string().into())).await.unwrap();
            if ending == "idle_read_error" {
                return;
            }
            let _ = upstream.next().await;
        });
        let fixture_storage = storage.clone();
        let app = Router::new().route(
            "/responses",
            get(move |upgrade: WebSocketUpgrade| {
                let storage = fixture_storage.clone();
                let supplier = supplier.clone();
                async move {
                    let (upstream, _) = tokio_tungstenite::client_async_tls_with_config(
                        format!("ws://{upstream_addr}/responses"),
                        tokio_tungstenite::MaybeTlsStream::Plain(
                            tokio::net::TcpStream::connect(upstream_addr).await.unwrap(),
                        ),
                        None,
                        None,
                    )
                    .await
                    .unwrap();
                    let ledger =
                        crate::usage::WsLedger::new(crate::execution::ExecutionContext::new(
                            storage.clone(),
                            &supplier,
                            "consumer",
                            "Consumer",
                            "/v1/responses",
                            "websocket",
                        ));
                    upgrade.on_upgrade(move |socket| {
                        bridge_recorded(
                            socket,
                            upstream,
                            "installation".into(),
                            false,
                            compaction.then(|| "Asia/Taipei".into()),
                            ledger,
                            (
                                storage,
                                AccessCheck {
                                    hash: codex2api_storage::hash_token("access"),
                                    account_id: supplier.id,
                                },
                            ),
                            None,
                        )
                    })
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let proxy = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let (mut client, _) =
                tokio_tungstenite::connect_async(format!("ws://{addr}/responses"))
                    .await
                    .unwrap();
            let mut request = json!({"type":"response.create","model":"gpt-6-astra","input":[]});
            if compaction {
                request["input"] = json!([{"type":"compaction_trigger"}]);
                request["previous_response_id"] = json!("previous");
            }
            let frame = if ending == "compaction_binary" {
                UpstreamMessage::Binary(serde_json::to_vec(&request).unwrap().into())
            } else {
                UpstreamMessage::Text(request.to_string().into())
            };
            client.send(frame).await.unwrap();
            let created: Value =
                serde_json::from_slice(&client.next().await.unwrap().unwrap().into_data()).unwrap();
            assert_eq!(created["type"], "response.created");
            if ending == "read_error_with_supplier_failure" {
                storage
                    .reject_supplier_auth(
                        &supplier_id,
                        storage
                            .supplier_auth_revision(&supplier_id)
                            .await
                            .unwrap()
                            .unwrap(),
                    )
                    .await
                    .unwrap();
            }
            let health_before = storage.supplier_health(&supplier_id).await.unwrap();
            let expected = match ending {
                "rate_event" | "quota_event" | "auth_event" | "unknown_limit_event" => {
                    complete.send(()).unwrap();
                    let received: Value =
                        serde_json::from_slice(&client.next().await.unwrap().unwrap().into_data())
                            .unwrap();
                    assert_eq!(Some(received), error_event);
                    assert!(matches!(
                        client.next().await.unwrap().unwrap(),
                        UpstreamMessage::Close(_)
                    ));
                    let health = storage.supplier_health(&supplier_id).await.unwrap();
                    assert_eq!(health.authentication_invalid, ending == "auth_event");
                    "failed"
                }
                "upstream_read_error"
                | "upstream_protocol_error"
                | "idle_read_error"
                | "read_error_with_supplier_failure" => {
                    complete.send(()).unwrap();
                    if ending == "idle_read_error" {
                        let completed: Value = serde_json::from_slice(
                            &client.next().await.unwrap().unwrap().into_data(),
                        )
                        .unwrap();
                        assert_eq!(completed["type"], "response.completed");
                    }
                    let rejected: Value =
                        serde_json::from_slice(&client.next().await.unwrap().unwrap().into_data())
                            .unwrap();
                    assert_eq!(rejected["type"], "error");
                    assert_eq!(rejected["status"], 502);
                    assert_eq!(rejected["error"]["code"], "stream_error");
                    match client.next().await.unwrap().unwrap() {
                        UpstreamMessage::Close(Some(frame)) => {
                            assert_eq!(u16::from(frame.code), 1011)
                        }
                        frame => panic!("expected a WebSocket close frame, got {frame:?}"),
                    }
                    let health_after = storage.supplier_health(&supplier_id).await.unwrap();
                    assert_eq!(health_after.error_message, health_before.error_message);
                    assert_eq!(health_after.error_at, health_before.error_at);
                    assert_eq!(health_after.revision, health_before.revision);
                    assert_eq!(
                        AccessCheck {
                            hash: codex2api_storage::hash_token("access"),
                            account_id: supplier_id.clone(),
                        }
                        .validate(&storage)
                        .await
                        .is_ok(),
                        ending != "read_error_with_supplier_failure"
                    );
                    if ending == "idle_read_error" {
                        "completed"
                    } else {
                        "failed"
                    }
                }
                "upstream_ping" => {
                    complete.send(()).unwrap();
                    let completed: Value =
                        serde_json::from_slice(&client.next().await.unwrap().unwrap().into_data())
                            .unwrap();
                    assert_eq!(completed["type"], "response.completed");
                    "completed"
                }
                "compaction_text" | "compaction_binary" => {
                    complete.send(()).unwrap();
                    let item: Value =
                        serde_json::from_slice(&client.next().await.unwrap().unwrap().into_data())
                            .unwrap();
                    assert_eq!(item["item"]["type"], "compaction");
                    assert_eq!(
                        item["item"]["encrypted_content"],
                        "fixture-compacted-history"
                    );
                    let completed: Value =
                        serde_json::from_slice(&client.next().await.unwrap().unwrap().into_data())
                            .unwrap();
                    assert_eq!(completed["type"], "response.completed");
                    "completed"
                }
                "client_closed" => {
                    client.close(None).await.unwrap();
                    complete.send(()).unwrap();
                    "client_stopped"
                }
                "upstream_closed" => {
                    complete.send(()).unwrap();
                    let event: Value =
                        serde_json::from_slice(&client.next().await.unwrap().unwrap().into_data())
                            .unwrap();
                    assert_eq!(event["status"], 502);
                    assert_eq!(event["error"]["code"], "upstream_websocket_closed");
                    assert!(matches!(
                        client.next().await.unwrap().unwrap(),
                        UpstreamMessage::Close(_)
                    ));
                    "failed"
                }
                _ => {
                    storage
                        .revoke_virtual_device("consumer", &device)
                        .await
                        .unwrap();
                    complete.send(()).unwrap();
                    let rejected: Value =
                        serde_json::from_slice(&client.next().await.unwrap().unwrap().into_data())
                            .unwrap();
                    assert_eq!(rejected["status"], 401);
                    assert!(matches!(
                        client.next().await.unwrap().unwrap(),
                        UpstreamMessage::Close(_)
                    ));
                    "completed"
                }
            };
            let records = loop {
                let records = storage.query_usage(&Default::default()).await.unwrap();
                if records
                    .records
                    .first()
                    .is_some_and(|record| record.status != "in_progress")
                {
                    break records;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            };
            assert_eq!(records.total, 1);
            assert_eq!(records.records[0].status, expected);
            if error_event.is_some() {
                let expected = match ending {
                    "rate_event" => "rate_limit",
                    "quota_event" => "quota_exhausted",
                    "auth_event" => "authentication",
                    _ => "limit_unknown",
                };
                assert_eq!(records.records[0].failure_kind.as_deref(), Some(expected));
                assert_eq!(
                    records.records[0].failure_status,
                    Some(if ending == "auth_event" { 401 } else { 429 })
                );
            } else if ending == "upstream_closed" {
                assert_eq!(
                    records.records[0].error_code.as_deref(),
                    Some("upstream_websocket_closed")
                );
                assert!(records.records[0].error_message.is_some());
            } else if read_error && ending != "idle_read_error" {
                assert_eq!(
                    records.records[0].error_code.as_deref(),
                    Some("upstream_websocket_read_error")
                );
                assert!(
                    records.records[0]
                        .error_message
                        .as_deref()
                        .unwrap()
                        .contains(if ending == "upstream_protocol_error" {
                            "Reserved bits are non-zero"
                        } else {
                            "Connection reset without closing handshake"
                        })
                );
            } else {
                assert!(records.records[0].error_message.is_none());
            }
            assert_eq!(records.records[0].input_tokens, Some(10));
            assert_eq!(records.records[0].output_tokens, Some(2));
            assert!(
                records.records[0]
                    .cost_nano_usd
                    .is_some_and(|cost| cost > 0)
            );
        })
        .await
        .unwrap();
        if ending == "upstream_read_error" {
            use axum::{body::Body, http::Request};
            use tower::ServiceExt;
            let admin =
                codex2api_admin::router(codex2api_admin::AdminState::new(storage.clone()).unwrap());
            let login = admin
                .clone()
                .oneshot(
                    Request::post("/admin/api/login")
                        .header("content-type", "application/json")
                        .body(Body::from(r#"{"username":"admin","password":"admin"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(login.status(), axum::http::StatusCode::OK);
            let cookie = login.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned();
            for path in [
                format!("/admin/api/suppliers/{supplier_id}"),
                format!("/admin/api/usage?supplier_id={supplier_id}"),
            ] {
                let response = admin
                    .clone()
                    .oneshot(
                        Request::get(&path)
                            .header("cookie", &cookie)
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), axum::http::StatusCode::OK);
                let value: Value = serde_json::from_slice(
                    &axum::body::to_bytes(response.into_body(), 1024 * 1024)
                        .await
                        .unwrap(),
                )
                .unwrap();
                if path.contains("/suppliers/") {
                    assert_eq!(value["status"], "active");
                    assert!(value["error_message"].is_null());
                } else {
                    assert_eq!(value["total"], 1);
                    assert_eq!(
                        value["records"][0]["error_code"],
                        "upstream_websocket_read_error"
                    );
                    assert!(
                        value["records"][0]["error_message"]
                            .as_str()
                            .unwrap()
                            .contains("Connection reset without closing handshake")
                    );
                }
            }
        }
        proxy.abort();
        if read_error {
            upstream_task.await.unwrap();
        } else {
            upstream_task.abort();
        }
        storage.close().await;
    }

    async fn transport_fixture(mut client: WebSocket, mut upstream: UpstreamWebSocket) {
        while let Some(Ok(message)) = client.next().await {
            let message = match message {
                Message::Text(text) => UpstreamMessage::Text(
                    prepare_realtime_message(&text, "account").unwrap().into(),
                ),
                Message::Binary(bytes) => UpstreamMessage::Binary(bytes),
                _ => break,
            };
            upstream.send(message).await.unwrap();
            match upstream.next().await.unwrap().unwrap() {
                UpstreamMessage::Text(text) => client
                    .send(Message::Text(text.to_string().into()))
                    .await
                    .unwrap(),
                UpstreamMessage::Binary(bytes) => {
                    client.send(Message::Binary(bytes)).await.unwrap()
                }
                _ => break,
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn exhausted_virtual_quota_sends_error_and_close_before_forwarding() {
        socket_policy_fixture(
            false,
            serde_json::json!({"type":"response.create","model":"gpt-test","input":[]}),
            429,
            "virtual_quota_exceeded",
            true,
        )
        .await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn realtime_checks_each_generation_and_closes_cleanly() {
        socket_policy_fixture(
            true,
            serde_json::json!({"type":"response.create","response":{"model":"gpt-test"}}),
            429,
            "virtual_quota_exceeded",
            true,
        )
        .await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn malformed_metadata_warmups_and_realtime_changes_never_bypass_model_authorization() {
        use serde_json::json;
        for frame in [
            json!({"type":"response.create","model":"forbidden","size":{}}),
            json!({"type":"response.create","model":"forbidden","reasoning":{"effort":7}}),
        ] {
            socket_policy_fixture(false, frame, 400, "", false).await;
        }
        for generate in [true, false] {
            socket_policy_fixture(
                false,
                json!({"type":"response.create","model":"forbidden","generate":generate}),
                403,
                "model_not_entitled",
                false,
            )
            .await;
        }
        for frame in [
            json!({"type":"session.update","session":{"model":"forbidden"}}),
            json!({"type":"response.create","response":{"model":"forbidden"}}),
        ] {
            socket_policy_fixture(true, frame, 403, "model_not_entitled", false).await;
        }
        socket_policy_fixture(true,json!({"type":"session.update","session":{"audio":{"input":{"transcription":{"model":"forbidden"}}}}}),403,"model_not_entitled",false).await;
        for frame in [
            json!({"type":"response.create","session":{"model":"gpt-test"},"response":{"model":"forbidden"}}),
            json!({"type":"session.update","model":"gpt-test"}),
            json!({"type":"session.update","session":{"audio":{"input":{"transcription":{}}}}}),
        ] {
            socket_policy_fixture(true, frame, 400, "", false).await;
        }
    }

    async fn socket_policy_fixture(
        realtime: bool,
        frame: serde_json::Value,
        expected_status: u16,
        expected_code: &str,
        exhausted: bool,
    ) {
        use axum::{Router, routing::get};
        use codex2api_storage::{OAuthDeviceIdentity, SupplierStatus, UsageRecord, VirtualAccount};
        use serde_json::{Value, json};
        let dir = tempfile::tempdir().unwrap();
        let storage = codex2api_storage::Storage::open(dir.path().join("quota.sqlite"))
            .await
            .unwrap();
        let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
        let real = accounts.create_pending().await.unwrap().account;
        storage
            .update_account(
                &real.id,
                codex2api_storage::SupplierAccountUpdate {
                    status: Some(SupplierStatus::Active),
                    chatgpt_account_id: Some(real.id.clone()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        accounts
            .save_auth_for_account(
                &real.id,
                &codex2api_accounts::AuthDotJson::chatgpt(
                    codex2api_accounts::TokenData {
                        id_token: "fixture".into(),
                        access_token: "fixture".into(),
                        refresh_token: "fixture".into(),
                        account_id: Some(real.id.clone()),
                    },
                    None,
                ),
            )
            .await
            .unwrap();
        let account = VirtualAccount {
            provider_id: "chatgpt".into(),
            id: "virtual".into(),
            username: "fixture".into(),
            password_hash: "unused".into(),
            name: "Virtual".into(),
            email: "v@example.test".into(),
            plan_type: "pro".into(),
            plan_id: "pro".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        storage.save_virtual_account(&account).await.unwrap();
        storage
            .save_execution_route(&account.id, "chatgpt", Some(&real.id), None)
            .await
            .unwrap();
        let device = storage
            .create_virtual_device(&account, "refresh", &OAuthDeviceIdentity::default())
            .await
            .unwrap()
            .unwrap();
        storage
            .register_virtual_access(
                &device,
                "refresh",
                "access",
                chrono::Utc::now().timestamp() + 600,
            )
            .await
            .unwrap();
        let mut plan = storage
            .virtual_plan(&account.plan_id)
            .await
            .unwrap()
            .unwrap();
        sqlx::query("INSERT OR IGNORE INTO model_catalog(provider_id,model,kind) VALUES('chatgpt','gpt-test','text')").execute(storage.pool()).await.unwrap();
        plan.config["model_access"] = json!("selected");
        plan.config["models"] = json!([{"provider_id":"chatgpt","model":"gpt-test"}]);
        plan.config["primary_cost_limit_usd"] = if exhausted { json!(0.01) } else { json!(null) };
        plan.config["weekly_cost_limit_usd"] = json!(null);
        assert!(
            storage
                .save_virtual_plan(&plan, Some(plan.revision))
                .await
                .unwrap()
        );
        let mut usage = UsageRecord {
            id: "used".into(),
            model: Some("gpt-6-astra".into()),
            endpoint: "/v1/responses".into(),
            account_id: real.id.clone(),
            subject_id: account.id.clone(),
            requested_at_ms: chrono::Utc::now().timestamp_millis(),
            status: "in_progress".into(),
            ..Default::default()
        };
        storage.insert_usage(&usage).await.unwrap();
        usage.input_tokens = Some(20701);
        usage.output_tokens = Some(27);
        usage.status = "completed".into();
        storage.finish_usage(&usage).await.unwrap();
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let forwarded = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = forwarded.clone();
        let server = tokio::spawn(async move {
            loop {
                let (stream, _) = upstream_listener.accept().await.unwrap();
                let count = count.clone();
                tokio::spawn(async move {
                    let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                    while let Some(Ok(message)) = socket.next().await {
                        if message.is_text() || message.is_binary() {
                            let value: serde_json::Value =
                                serde_json::from_slice(&message.into_data()).unwrap();
                            if !realtime
                                && value.get("generate") == Some(&serde_json::Value::Bool(false))
                            {
                                // Native Desktop performs a non-billable connection warmup.
                                socket.send(UpstreamMessage::Text(json!({"type":"response.completed","response":{"id":"warmup","status":"completed"}}).to_string().into())).await.unwrap();
                            } else {
                                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            }
                        }
                    }
                });
            }
        });
        let fixture_storage = storage.clone();
        let app = Router::new().route(
            "/responses",
            get(move |upgrade: WebSocketUpgrade| {
                let storage = fixture_storage.clone();
                let real = real.clone();
                async move {
                    let (upstream, _) = tokio_tungstenite::client_async_tls_with_config(
                        format!("ws://{upstream_addr}/responses"),
                        tokio_tungstenite::MaybeTlsStream::Plain(
                            tokio::net::TcpStream::connect(upstream_addr).await.unwrap(),
                        ),
                        None,
                        None,
                    )
                    .await
                    .unwrap();
                    let context = crate::execution::ExecutionContext::new(
                        storage.clone(),
                        &real,
                        "virtual",
                        "Virtual",
                        if realtime {
                            "/v1/realtime"
                        } else {
                            "/v1/responses"
                        },
                        "websocket",
                    );
                    let ledger = if realtime {
                        crate::usage::WsLedger::realtime(context, "gpt-test".into())
                    } else {
                        crate::usage::WsLedger::new(context)
                    };
                    upgrade.on_upgrade(move |socket| {
                        bridge_recorded(
                            socket,
                            upstream,
                            "installation".into(),
                            realtime,
                            None,
                            ledger,
                            (
                                storage,
                                AccessCheck {
                                    hash: codex2api_storage::hash_token("access"),
                                    account_id: real.id,
                                },
                            ),
                            None,
                        )
                    })
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let proxy = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let (mut client, _)=tokio_tungstenite::connect_async(format!("ws://{addr}/responses")).await.unwrap();
            client.send(UpstreamMessage::Text(frame.to_string().into())).await.unwrap();
            let message=client.next().await.unwrap().unwrap();
            let event:Value=serde_json::from_slice(&message.into_data()).unwrap();
            assert_eq!(event["type"],"error");
            assert_eq!(event["status"],expected_status);
            if !expected_code.is_empty() {assert_eq!(event["error"]["code"],expected_code);}
            assert!(matches!(client.next().await.unwrap().unwrap(),UpstreamMessage::Close(Some(frame)) if frame.code==tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Policy));
            let _=client.close(None).await;
        }).await.unwrap();
        if exhausted && !realtime && std::env::var_os("CODEX2API_TEST_CLI").is_some() {
            let url = format!("http://{addr}");
            let output = tokio::task::spawn_blocking(move || {
                std::process::Command::new("python")
                    .arg(
                        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../scripts/windows/Test-DesktopResponseOutcomes.py"),
                    )
                    .arg(url)
                    .arg("429")
                    .arg("websocket")
                    .output()
                    .unwrap()
            })
            .await
            .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        assert_eq!(forwarded.load(std::sync::atomic::Ordering::SeqCst), 0);
        proxy.abort();
        server.abort();
        storage.close().await;
    }

    #[test]
    fn realtime_session_and_audio_events_are_unchanged_except_installation_identity() {
        let audio = r#"{ "type": "input_audio_buffer.append", "audio": "raw==" }"#;
        assert_eq!(prepare_realtime_message(audio, "account").unwrap(), audio);
        let original = serde_json::json!({"type":"session.update", "session":{"id":"session-id", "instructions":"preserve", "client_metadata":{"x-codex-installation-id":"caller"}}});
        let output: serde_json::Value = serde_json::from_str(
            &prepare_realtime_message(&original.to_string(), "account").unwrap(),
        )
        .unwrap();
        let mut expected = original;
        expected["session"]["client_metadata"]["x-codex-installation-id"] = "account".into();
        assert_eq!(output, expected);
    }

    #[tokio::test]
    async fn realtime_transport_preserves_binary_audio_and_identity() {
        use axum::{Router, routing::get};
        let dir = tempfile::tempdir().unwrap();
        let storage = codex2api_storage::Storage::open(dir.path().join("websocket.sqlite"))
            .await
            .unwrap();
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = upstream_listener.accept().await.unwrap();
            let mut upstream = tokio_tungstenite::accept_async(stream).await.unwrap();
            let message = upstream.next().await.unwrap().unwrap();
            assert_eq!(message.clone().into_data().as_ref(), &[0xff, 0, 128, 1]);
            upstream.send(message).await.unwrap();
            let message = upstream.next().await.unwrap().unwrap();
            upstream.send(message).await.unwrap();
        });
        let app = Router::new().route(
            "/live",
            get(move |upgrade: WebSocketUpgrade| async move {
                let (upstream, _) = tokio_tungstenite::client_async_tls_with_config(
                    format!("ws://{upstream_addr}/live"),
                    tokio_tungstenite::MaybeTlsStream::Plain(
                        tokio::net::TcpStream::connect(upstream_addr).await.unwrap(),
                    ),
                    None,
                    None,
                )
                .await
                .unwrap();
                upgrade.on_upgrade(move |socket| transport_fixture(socket, upstream))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let proxy = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let future = async {
            let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/live"))
                .await
                .unwrap();
            client
                .send(UpstreamMessage::Binary(vec![0xff, 0, 128, 1].into()))
                .await
                .unwrap();
            assert_eq!(
                client.next().await.unwrap().unwrap().into_data().as_ref(),
                &[0xff, 0, 128, 1]
            );
            client.send(UpstreamMessage::Text(serde_json::json!({"type":"session.update", "session":{"client_metadata":{"x-codex-installation-id":"caller"}}}).to_string().into())).await.unwrap();
            let message = client.next().await.unwrap().unwrap();
            let value: serde_json::Value = serde_json::from_slice(&message.into_data()).unwrap();
            assert_eq!(
                value["session"]["client_metadata"]["x-codex-installation-id"],
                "account"
            );
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), future)
            .await
            .unwrap();
        server.await.unwrap();
        proxy.abort();
        storage.close().await;
    }

    #[test]
    fn timezone_keeps_incremental_compaction_last_and_preserves_session_metadata() {
        let original = serde_json::json!({
            "type":"response.create", "model":"gpt-6-astra", "previous_response_id":"previous",
            "input":[{"type":"compaction_trigger"}],
            "client_metadata":{"x-codex-installation-id":"caller","turn_id":"turn","session_id":"session"}
        });
        let prepared: serde_json::Value = serde_json::from_str(
            &prepare_message(
                &original.to_string(),
                "supplier-installation",
                Some("Asia/Taipei"),
                false,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(prepared["input"][1], original["input"][0]);
        assert!(
            prepared["input"][0]
                .to_string()
                .contains("<timezone>Asia/Taipei</timezone>")
        );
        assert_eq!(prepared["previous_response_id"], "previous");
        assert_eq!(prepared["client_metadata"]["session_id"], "session");
        assert_eq!(prepared["client_metadata"]["turn_id"], "turn");
        assert_eq!(
            prepared["client_metadata"]["x-codex-installation-id"],
            "supplier-installation"
        );
    }

    #[test]
    fn keeps_incremental_websocket_session_data() {
        let original = serde_json::json!({"type":"response.create", "previous_response_id":"previous",
            "generate":false, "input":[{"content":"do not change"}],
            "client_metadata":{"x-codex-installation-id":"caller", "turn_id":"turn", "session_id":"session"}});
        let out: serde_json::Value = serde_json::from_str(
            &prepare_message(&original.to_string(), "account", None, false).unwrap(),
        )
        .unwrap();
        let mut expected = original;
        expected["client_metadata"]["x-codex-installation-id"] = "account".into();
        assert_eq!(out, expected);
        let configured: serde_json::Value = serde_json::from_str(
            &prepare_message(&expected.to_string(), "account", Some("Asia/Taipei"), false).unwrap(),
        )
        .unwrap();
        assert_eq!(configured["previous_response_id"], "previous");
        assert!(
            configured["input"]
                .to_string()
                .contains("<timezone>Asia/Taipei</timezone>")
        );
    }
}

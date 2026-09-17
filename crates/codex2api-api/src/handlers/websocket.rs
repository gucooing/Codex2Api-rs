use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::extract::{Extension, State, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as UpstreamMessage;

use crate::auth::authenticate;
use crate::{ApiState, Result};
use codex2api_upstream::{
    Endpoint, UpstreamWebSocket, normalize_response_identity, strip_hop_by_hop_headers,
};

pub async fn responses_websocket(
    State(state): State<ApiState>,
    Extension(endpoint): Extension<Endpoint>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    let (key, ctx) = authenticate(&state, &headers).await?;
    let ledger = crate::usage::WsLedger::new(crate::usage::UsageContext::new(
        state.storage.clone(),
        &ctx.account,
        &key,
        &format!("/v1/{}", endpoint.codex_path()),
        "websocket",
    ));
    let upstream = state.upstream.get(&ctx.account.id).await?;
    let (socket, mut response_headers) = upstream.connect_websocket(endpoint, headers).await?;
    let installation_id = upstream.identity().installation_id.clone();
    let timezone = upstream.identity().http_fingerprint.timezone.clone();
    strip_hop_by_hop_headers(&mut response_headers);
    for name in [
        "sec-websocket-accept",
        "sec-websocket-extensions",
        "sec-websocket-protocol",
        "content-length",
        "set-cookie",
    ] {
        response_headers.remove(name);
    }
    let mut response = upgrade
        .max_message_size(codex2api_upstream::MAX_REQUEST_BYTES)
        .on_upgrade(move |client| {
            bridge_recorded(
                client,
                socket,
                installation_id,
                false,
                timezone,
                Some(ledger),
                Some((state.storage, key.key_hash)),
            )
        });
    response.headers_mut().extend(response_headers);
    Ok(response)
}

fn prepare_message(
    text: &str,
    installation_id: &str,
    timezone: Option<&str>,
) -> codex2api_upstream::Result<String> {
    let mut value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| codex2api_upstream::UpstreamError::InvalidRequest(e.to_string()))?;
    if value.get("type").and_then(serde_json::Value::as_str) == Some("response.create") {
        codex2api_upstream::apply_response_timezone(&mut value, timezone)?;
        normalize_response_identity(&mut value, installation_id, &HeaderMap::new())?;
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
    if let Some(session) = value.get_mut("session") {
        if session.get("client_metadata").is_some() {
            normalize_response_identity(session, installation_id, &HeaderMap::new())?;
            changed = true;
        }
    }
    if changed {
        Ok(serde_json::to_string(&value)?)
    } else {
        Ok(text.to_string())
    }
}

pub(crate) async fn bridge(
    client: WebSocket,
    upstream: UpstreamWebSocket,
    installation_id: String,
    realtime: bool,
    key_access: Option<(codex2api_storage::Storage, String)>,
) {
    bridge_recorded(
        client,
        upstream,
        installation_id,
        realtime,
        None,
        None,
        key_access,
    )
    .await
}

async fn bridge_recorded(
    client: WebSocket,
    upstream: UpstreamWebSocket,
    installation_id: String,
    realtime: bool,
    timezone: Option<String>,
    ledger: Option<crate::usage::WsLedger>,
    key_access: Option<(codex2api_storage::Storage, String)>,
) {
    let ledger = ledger.map(tokio::sync::Mutex::new);
    let (mut client_tx, mut client_rx) = client.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();
    let to_upstream = async {
        while let Some(message) = client_rx.next().await {
            let message = message.map_err(|e| e.to_string())?;
            if matches!(&message, Message::Text(_) | Message::Binary(_))
                && let Some((storage, hash)) = &key_access
                && storage
                    .lookup_proxy_api_key_by_hash(hash)
                    .await
                    .map_err(|error| error.to_string())?
                    .is_none()
            {
                return Err("API Key is paused or deleted".to_string());
            }
            let message = match message {
                Message::Text(text) => UpstreamMessage::Text(
                    (if realtime {
                        prepare_realtime_message(&text, &installation_id)
                    } else {
                        prepare_message(&text, &installation_id, timezone.as_deref())
                    })
                    .map_err(|e| e.to_string())?
                    .into(),
                ),
                Message::Binary(bytes) if realtime => UpstreamMessage::Binary(bytes),
                Message::Binary(bytes) => {
                    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
                    UpstreamMessage::Text(
                        prepare_message(text, &installation_id, timezone.as_deref())
                            .map_err(|e| e.to_string())?
                            .into(),
                    )
                }
                Message::Close(frame) => {
                    upstream_tx
                        .send(UpstreamMessage::Close(frame.map(|f| {
                            tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                code: f.code.into(),
                                reason: f.reason.to_string().into(),
                            }
                        })))
                        .await
                        .map_err(|e| e.to_string())?;
                    return Ok::<_, String>(());
                }
                Message::Ping(_) | Message::Pong(_) => continue,
            };
            if let (Some(ledger), UpstreamMessage::Text(text)) = (&ledger, &message) {
                crate::usage::ws_start(ledger, text.as_str())
                    .await
                    .map_err(|e| e.to_string())?;
            }
            upstream_tx.send(message).await.map_err(|e| e.to_string())?;
        }
        let _ = upstream_tx.close().await;
        Ok(())
    };
    let to_client = async {
        while let Some(message) = upstream_rx.next().await {
            let message = message.map_err(|e| e.to_string())?;
            if let Some(ledger) = &ledger {
                match &message {
                    UpstreamMessage::Text(text) => ledger.lock().await.observe(text.as_bytes()),
                    UpstreamMessage::Binary(bytes) => ledger.lock().await.observe(bytes),
                    _ => {}
                }
            }
            let message = match message {
                UpstreamMessage::Text(text) => Message::Text(text.to_string().into()),
                UpstreamMessage::Binary(bytes) => Message::Binary(bytes),
                UpstreamMessage::Close(frame) => {
                    client_tx
                        .send(Message::Close(frame.map(|f| CloseFrame {
                            code: f.code.into(),
                            reason: f.reason.to_string().into(),
                        })))
                        .await
                        .map_err(|e| e.to_string())?;
                    return Ok::<_, String>(());
                }
                UpstreamMessage::Ping(_) | UpstreamMessage::Pong(_) | UpstreamMessage::Frame(_) => {
                    continue;
                }
            };
            client_tx.send(message).await.map_err(|e| e.to_string())?;
        }
        let _ = client_tx.close().await;
        Ok(())
    };
    let result = tokio::select! { result = to_upstream => result, result = to_client => result };
    if let Err(error) = result {
        tracing::debug!(%error, "Responses WebSocket relay ended");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn realtime_bridge_keeps_existing_connection_and_identity_after_rebinding() {
        use axum::{Router, routing::get};
        let dir = tempfile::tempdir().unwrap();
        let storage = codex2api_storage::Storage::open(dir.path().join("websocket.sqlite"))
            .await
            .unwrap();
        let accounts = codex2api_accounts::AccountStore::open(storage.clone());
        let first = accounts.create_pending().await.unwrap().account;
        let second = accounts.create_pending().await.unwrap().account;
        storage
            .set_account_status(&second.id, codex2api_storage::AccountStatus::Active)
            .await
            .unwrap();
        let key = storage.create_proxy_api_key(&first.id, None).await.unwrap();
        let key_access = (storage.clone(), key.record.key_hash.clone());
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
            get(move |upgrade: WebSocketUpgrade| {
                let key_access = key_access.clone();
                async move {
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
                    upgrade.on_upgrade(move |socket| {
                        bridge(socket, upstream, "account".into(), true, Some(key_access))
                    })
                }
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
            assert!(
                storage
                    .bind_proxy_api_key(&key.record.id, &second.id)
                    .await
                    .unwrap()
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
    fn keeps_incremental_websocket_session_data() {
        let original = serde_json::json!({"type":"response.create", "previous_response_id":"previous",
            "generate":false, "input":[{"content":"do not change"}],
            "client_metadata":{"x-codex-installation-id":"caller", "turn_id":"turn", "session_id":"session"}});
        let out: serde_json::Value =
            serde_json::from_str(&prepare_message(&original.to_string(), "account", None).unwrap())
                .unwrap();
        let mut expected = original;
        expected["client_metadata"]["x-codex-installation-id"] = "account".into();
        assert_eq!(out, expected);
        let configured: serde_json::Value = serde_json::from_str(
            &prepare_message(&expected.to_string(), "account", Some("Asia/Taipei")).unwrap(),
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

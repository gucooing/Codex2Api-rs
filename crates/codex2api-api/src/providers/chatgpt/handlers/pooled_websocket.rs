use crate::{
    ApiError, ApiState, Result, execution::ExecutionContext, pool_execution as pool,
    usage::WsLedger,
};
use axum::{
    extract::{
        WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    response::Response,
};
use codex2api_accounts::SupplierContext;
use codex2api_storage::VirtualAccess;
use codex2api_upstream::{Endpoint, ResponseOutcome, UpstreamWebSocket};
use futures::{SinkExt, StreamExt};
use http::HeaderMap;
use serde_json::Value;
use std::{collections::VecDeque, time::Instant};
use tokio_tungstenite::tungstenite::Message as UpstreamMessage;

struct Connection {
    socket: UpstreamWebSocket,
    ctx: SupplierContext,
    headers: HeaderMap,
    revision: Option<i64>,
    workspace: Option<codex2api_upstream::WorkspaceConnection>,
}

fn context(
    state: &ApiState,
    oauth: &VirtualAccess,
    conn: &Connection,
    endpoint: Endpoint,
) -> ExecutionContext {
    ExecutionContext::new(
        state.storage.clone(),
        &conn.ctx.account,
        &oauth.virtual_account_id,
        &oauth.name,
        &format!("/v1/{}", endpoint.codex_path()),
        "websocket",
    )
}

async fn open(
    state: &ApiState,
    oauth: &VirtualAccess,
    endpoint: Endpoint,
    headers: &HeaderMap,
    excluded: &mut Vec<String>,
) -> Result<Connection> {
    loop {
        let ctx = pool::select(state, oauth, excluded).await?;
        let revision = state
            .storage
            .supplier_auth_revision(&ctx.account.id)
            .await?;
        let attempt = async {
            #[cfg(test)]
            if let Some(url) = state.websocket_test_targets.get(&ctx.account.id) {
                let socket = tests::fixture_socket(url).await;
                return Ok((socket, HeaderMap::new(), None));
            }
            let upstream = state.upstream.get(&ctx.account.id).await?;
            let mut inbound = headers.clone();
            if oauth.account_id.as_deref() != Some(ctx.account.id.as_str()) {
                inbound.remove("x-codex-turn-state");
            }
            upstream.connect_websocket(endpoint, inbound).await
        }
        .await;
        match attempt {
            Ok((socket, headers, workspace)) => {
                let revision = workspace
                    .as_ref()
                    .map(codex2api_upstream::WorkspaceConnection::auth_revision)
                    .or(revision);
                return Ok(Connection {
                    socket,
                    ctx,
                    headers,
                    revision,
                    workspace,
                });
            }
            Err(error) => {
                let Some(failure) = error.supplier_failure(chrono::Utc::now().timestamp()) else {
                    return Err(error.into());
                };
                pool::observe(state, &ctx.account.id, revision, &failure).await?;
                excluded.push(ctx.account.id);
            }
        }
    }
}

pub(super) async fn connect(
    state: ApiState,
    endpoint: Endpoint,
    oauth: VirtualAccess,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    super::super::access::resolve_supplier(&state, &headers, axum::Extension(oauth.clone()))
        .await?;
    let service = codex2api_service::ExecutionService::new(state.storage.clone());
    service.check_budget(&oauth.virtual_account_id).await?;
    service
        .admit_request(&oauth.virtual_account_id, false)
        .await?;
    let connection = open(&state, &oauth, endpoint, &headers, &mut Vec::new()).await?;
    let mut response_headers = connection.headers.clone();
    super::super::identity::quota_headers(
        &state.storage,
        &oauth.virtual_account_id,
        &mut response_headers,
    )
    .await?;
    codex2api_upstream::strip_hop_by_hop_headers(&mut response_headers);
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
        .on_upgrade(move |client| bridge(client, connection, state, oauth, endpoint, headers));
    response.headers_mut().extend(response_headers);
    Ok(response)
}

struct Pending {
    original: Value,
    full_input: Option<Vec<Value>>,
    prelude: Vec<(String, Instant)>,
    prelude_bytes: usize,
    committed: bool,
    excluded: Vec<String>,
    refreshed: std::collections::HashSet<String>,
}

async fn send(conn: &mut Connection, value: &Value, reviewer: bool) -> Result<()> {
    if let Some(workspace) = &conn.workspace {
        workspace.check_current().await?;
    }
    let text = super::websocket::prepare_message(
        &value.to_string(),
        &conn.ctx.identity.installation_id,
        conn.ctx.identity.http_fingerprint.timezone.as_deref(),
        reviewer,
    )?;
    conn.socket
        .send(UpstreamMessage::Text(text.into()))
        .await
        .map_err(super::websocket::relay_error)
}

async fn deliver(
    client: &mut WebSocket,
    ledger: &tokio::sync::Mutex<WsLedger>,
    state: &ApiState,
    oauth: &VirtualAccess,
    text: &str,
    at: Instant,
) -> Result<Option<ResponseOutcome>> {
    let outcome = ledger
        .lock()
        .await
        .observe_at(text.as_bytes(), true, at)
        .await?;
    let text =
        super::super::identity::websocket_message(&state.storage, &oauth.token_hash, text).await?;
    client
        .send(Message::Text(text.into()))
        .await
        .map_err(super::websocket::relay_error)?;
    Ok(outcome)
}

async fn bridge(
    mut client: WebSocket,
    mut conn: Connection,
    state: ApiState,
    oauth: VirtualAccess,
    endpoint: Endpoint,
    headers: HeaderMap,
) {
    let mut ledger = WsLedger::new(context(&state, &oauth, &conn, endpoint));
    ledger.response_headers(&conn.headers);
    let ledger = tokio::sync::Mutex::new(ledger);
    let reviewer = endpoint == Endpoint::Guardian
        || headers
            .get("x-codex-guardian")
            .is_some_and(|h| h == "reviewer");
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut pending: Option<Pending> = None;
    // In-memory connection context only; never written to logs or shared accounts.
    let mut history: Option<(String, Vec<Value>)> = None;
    let mut close_code = 1000;
    let result: Result<()> = async {
        loop {
            if pending.is_none() && let Some(text) = queue.pop_front() {
                let mut original: Value = serde_json::from_str(&text).map_err(|_| ApiError::bad_request("Invalid Responses frame."))?;
                let selected = pool::select(&state,&oauth,&[]).await?;
                let switched = selected.account.id != conn.ctx.account.id;
                if switched {
                    let _ = conn.socket.close(None).await;
                    conn = open(&state,&oauth,endpoint,&headers,&mut Vec::new()).await?;
                    ledger.lock().await.retry_supplier(context(&state,&oauth,&conn,endpoint),&conn.ctx.account).await?;
                    ledger.lock().await.response_headers(&conn.headers);
                }
                let full_input = original.get("input").and_then(Value::as_array).and_then(|input| {
                    match original.get("previous_response_id").and_then(Value::as_str) {
                        None => Some(input.clone()),
                        Some(id) => history.as_ref().filter(|(previous,_)| previous==id).map(|(_,old)| old.iter().chain(input).cloned().collect()),
                    }
                });
                if switched && original.get("previous_response_id").is_some_and(|v| !v.is_null()) {
                    let input = full_input.as_ref().ok_or_else(||ApiError::openai(http::StatusCode::CONFLICT,"configuration_error","Reconnect with the full conversation input after a supplier change.",Some("supplier_context_required")))?;
                    original.as_object_mut().unwrap().remove("previous_response_id");
                    original["input"] = serde_json::json!(input);
                }
                crate::usage::ws_start(&ledger,&text).await?;
                send(&mut conn,&original,reviewer).await?;
                pending = Some(Pending { original, full_input, prelude:Vec::new(),prelude_bytes:0,committed:false,excluded:Vec::new(),refreshed:Default::default() });
            }
            tokio::select! {
                incoming = client.next() => {
                    let Some(message) = incoming else { ledger.lock().await.client_stopped().await?; return Ok(()); };
                    let message = message.map_err(super::websocket::relay_error)?;
                    match message {
                        Message::Text(_) | Message::Binary(_) => {
                            state.storage.virtual_access(&oauth.token_hash).await?.ok_or_else(ApiError::invalid_token)?;
                            let text = match message { Message::Text(t)=>t.to_string(), Message::Binary(b)=>String::from_utf8(b.to_vec()).map_err(|_|ApiError::bad_request("Expected UTF-8 Responses frame."))?, _=>unreachable!() };
                            let value: Value = serde_json::from_str(&text).map_err(|_|ApiError::bad_request("Invalid Responses frame."))?;
                            if value["type"] == "response.create" {
                                if queue.len() >= 32 || queue.iter().map(String::len).sum::<usize>() + text.len() > codex2api_upstream::MAX_REQUEST_BYTES {
                                    return Err(ApiError::bad_request("Too many queued Responses requests."));
                                }
                                queue.push_back(text);
                            } else { send(&mut conn,&value,reviewer).await?; }
                        }
                        Message::Close(_) => { ledger.lock().await.client_stopped().await?; let _=conn.socket.close(None).await; return Ok(()); }
                        Message::Ping(_) | Message::Pong(_) => {}
                    }
                }
                incoming = conn.socket.next() => {
                    let Some(message) = incoming else { return Err(super::websocket::relay_error("Upstream WebSocket closed")); };
                    let message = message.map_err(super::websocket::relay_error)?;
                    let text = match message {
                        UpstreamMessage::Text(t)=>t.to_string(),
                        UpstreamMessage::Binary(b)=>String::from_utf8(b.to_vec()).map_err(|_|ApiError::bad_request("Expected UTF-8 Responses frame."))?,
                        UpstreamMessage::Close(_) => { if pending.is_some(){ return Err(super::websocket::relay_error("Upstream WebSocket closed")); } return Ok(()); },
                        _=>continue,
                    };
                    if state.storage.virtual_access(&oauth.token_hash).await?.is_none() {
                        // Actual completed usage survives a concurrent device revocation.
                        ledger.lock().await.observe_at(text.as_bytes(),false,Instant::now()).await?;
                        return Err(ApiError::invalid_token());
                    }
                    let value: Value = serde_json::from_str(&text).map_err(|_|super::websocket::relay_error("Invalid upstream Responses event"))?;
                    if let Some(mut failure) = codex2api_upstream::classify_supplier_failure(None,&value,&conn.headers,chrono::Utc::now().timestamp()) {
                        let recovered = if failure == codex2api_upstream::SupplierFailure::Authentication
                            && pending.as_ref().is_some_and(|p| !p.committed && !p.refreshed.contains(&conn.ctx.account.id)) {
                            pending.as_mut().unwrap().refreshed.insert(conn.ctx.account.id.clone());
                            match pool::recover_stream_auth(&state, &conn.ctx, conn.revision).await? {
                                None => true,
                                Some(rejection) => { failure = rejection; false }
                            }
                        } else { false };
                        if !recovered { pool::observe(&state,&conn.ctx.account.id,conn.revision,&failure).await?; }
                        if let Some(p) = &mut pending && !p.committed {
                            if !recovered { p.excluded.push(conn.ctx.account.id.clone()); }
                            let _=conn.socket.close(None).await;
                            conn=open(&state,&oauth,endpoint,&headers,&mut p.excluded).await?;
                            let mut retry=p.original.clone();
                            if retry.get("previous_response_id").is_some_and(|v|!v.is_null()) {
                                let input=p.full_input.as_ref().ok_or_else(||ApiError::openai(http::StatusCode::CONFLICT,"configuration_error","Reconnect with the full conversation input after a supplier change.",Some("supplier_context_required")))?;
                                retry.as_object_mut().unwrap().remove("previous_response_id");
                                retry["input"]=serde_json::json!(input);
                            }
                            p.prelude.clear(); p.prelude_bytes=0;
                            ledger.lock().await.retry_supplier(context(&state,&oauth,&conn,endpoint),&conn.ctx.account).await?;
                            ledger.lock().await.response_headers(&conn.headers);
                            send(&mut conn,&retry,reviewer).await?;
                            continue;
                        }
                        if pending.is_none() {
                            let mut excluded=vec![conn.ctx.account.id.clone()];
                            conn=open(&state,&oauth,endpoint,&headers,&mut excluded).await?;
                            ledger.lock().await.retry_supplier(context(&state,&oauth,&conn,endpoint),&conn.ctx.account).await?;
                            continue;
                        }
                    }
                    let at=Instant::now();
                    if let Some(p)=&mut pending {
                        if !p.committed && pool::prelude_event(&value) && p.prelude_bytes+text.len()<=256*1024 {
                            p.prelude_bytes+=text.len(); p.prelude.push((text,at)); continue;
                        }
                        for (text,at) in p.prelude.drain(..) { deliver(&mut client,&ledger,&state,&oauth,&text,at).await?; }
                        p.committed=true;
                    }
                    let outcome=deliver(&mut client,&ledger,&state,&oauth,&text,at).await?;
                    if let Some(outcome)=outcome {
                        if value["type"]=="response.completed" && let Some(p)=&pending && let Some(mut input)=p.full_input.clone() && let Some(id)=value.pointer("/response/id").and_then(Value::as_str) {
                            input.extend(value.pointer("/response/output").and_then(Value::as_array).cloned().unwrap_or_default());
                            history=(serde_json::to_vec(&input).is_ok_and(|v|v.len()<=codex2api_upstream::MAX_REQUEST_BYTES)).then(||(id.into(),input));
                        }
                        pending=None;
                        if let ResponseOutcome::Failed(failure) = outcome {
                            close_code = if failure.status.is_some_and(|status|status<500) {1008} else {1011};
                            return Ok(());
                        }
                    }
                }
            }
        }
    }.await;
    if let Err(error) = result {
        let (status, message) = super::websocket::error_message(error).await;
        close_code = if status < 500 { 1008 } else { 1011 };
        let value: Value = serde_json::from_str(&message).unwrap_or_default();
        let failure = codex2api_upstream::ResponseFailure::from_error(Some(status), &value);
        if let Err(error) = ledger.lock().await.fail_pending(failure).await {
            tracing::error!(%error,"failed to settle WebSocket generation");
        }
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.send(Message::Text(message.into())),
        )
        .await;
    }
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        client.send(Message::Close(Some(CloseFrame {
            code: close_code,
            reason: "Responses connection ended".into(),
        }))),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub(super) async fn fixture_socket(url: &str) -> UpstreamWebSocket {
        let parsed = reqwest::Url::parse(url).unwrap();
        let tcp =
            tokio::net::TcpStream::connect((parsed.host_str().unwrap(), parsed.port().unwrap()))
                .await
                .unwrap();
        tokio_tungstenite::client_async_tls_with_config(
            url,
            tokio_tungstenite::MaybeTlsStream::Plain(tcp),
            None,
            None,
        )
        .await
        .unwrap()
        .0
    }

    #[tokio::test]
    async fn rejected_prelude_is_consumed_and_exhausted_pool_sends_service_error_and_close() {
        pool_websocket_fixture(false, false).await;
    }

    #[tokio::test]
    async fn quota_rejection_switches_supplier_without_leaking_failure_or_double_admission() {
        pool_websocket_fixture(true, false).await;
    }

    #[tokio::test]
    async fn request_throttle_is_forwarded_without_rebinding_or_supplier_cooldown() {
        pool_websocket_fixture(true, true).await;
    }

    async fn pool_websocket_fixture(available: bool, throttle: bool) {
        let (_dir, mut state, oauth, ids) = crate::pool_execution::tests::setup_pool().await;
        state
            .storage
            .sync_supported_models(&codex2api_upstream::supported_models())
            .await
            .unwrap();
        if !available {
            state
                .storage
                .edit_supplier_tags(&[ids[1].clone()], &["pool".into()], true)
                .await
                .unwrap();
        }
        let second_task = if available {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            state
                .websocket_test_targets
                .insert(ids[1].clone(), format!("ws://{address}/responses"));
            let installation = state
                .accounts
                .load_context(&ids[1])
                .await
                .unwrap()
                .identity
                .installation_id;
            Some(tokio::spawn(async move {
                let (tcp, _) = listener.accept().await.unwrap();
                let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
                let frame = socket.next().await.unwrap().unwrap();
                let value: Value = serde_json::from_slice(&frame.into_data()).unwrap();
                assert_eq!(
                    value["client_metadata"]["x-codex-installation-id"],
                    installation
                );
                for value in [
                    json!({"type":"response.created","response":{"id":"winning-attempt"}}),
                    json!({"type":"response.completed","response":{"id":"winning-attempt","usage":{"input_tokens":7,"output_tokens":3},"output":[]}}),
                ] {
                    socket
                        .send(UpstreamMessage::Text(value.to_string().into()))
                        .await
                        .unwrap();
                }
                let _ = socket.next().await;
            }))
        } else {
            None
        };
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            let (tcp, _) = upstream_listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let frame = socket.next().await.unwrap().unwrap();
            let value: Value = serde_json::from_slice(&frame.into_data()).unwrap();
            assert_eq!(value["type"], "response.create");
            socket
                .send(UpstreamMessage::Text(
                    json!({"type":"response.created","response":{"id":"rejected-attempt"}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            socket.send(UpstreamMessage::Text(json!({"type":"response.failed","response":{"id":"rejected-attempt","error":{"code":if throttle {"rate_limit_exceeded"} else {"usage_limit_reached"},"message":"Please try again in 7s.","resets_at":chrono::Utc::now().timestamp()+600}},"headers":{"retry-after":"7"}}).to_string().into())).await.unwrap();
            let _ = socket.next().await;
        });
        let fixture_state = state.clone();
        let fixture_oauth = oauth.clone();
        let first = ids[0].clone();
        let app = axum::Router::new().route(
            "/responses",
            axum::routing::get(move |upgrade: WebSocketUpgrade| {
                let state = fixture_state.clone();
                let oauth = fixture_oauth.clone();
                let first = first.clone();
                async move {
                    let socket = fixture_socket(&format!("ws://{upstream_addr}/responses")).await;
                    let ctx = state.accounts.load_context(&first).await.unwrap();
                    let revision = state.storage.supplier_auth_revision(&first).await.unwrap();
                    let connection = Connection {
                        socket,
                        ctx,
                        headers: HeaderMap::new(),
                        revision,
                        workspace: None,
                    };
                    upgrade.on_upgrade(move |client| {
                        bridge(
                            client,
                            connection,
                            state,
                            oauth,
                            Endpoint::Responses,
                            HeaderMap::new(),
                        )
                    })
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app_task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/responses"))
            .await
            .unwrap();
        client
            .send(UpstreamMessage::Text(
                json!({"type":"response.create","model":"gpt-5.5","input":[]})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), client.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let value: Value = serde_json::from_slice(&event.into_data()).unwrap();
        if throttle {
            assert_eq!(value["type"], "response.created");
            assert_eq!(value["response"]["id"], "rejected-attempt");
            let frame = client.next().await.unwrap().unwrap();
            let error: Value = serde_json::from_slice(&frame.into_data()).unwrap();
            assert_eq!(error["response"]["error"]["code"], "rate_limit_exceeded");
            assert_eq!(error["headers"]["retry-after"], "7");
            assert!(matches!(
                client.next().await.unwrap().unwrap(),
                UpstreamMessage::Close(Some(_))
            ));
            let health = state.storage.supplier_health(&ids[0]).await.unwrap();
            assert!(!health.authentication_invalid);
            assert!(health.cooldown_kind.is_none());
            assert_eq!(
                state
                    .storage
                    .execution_route(&oauth.virtual_account_id, "chatgpt")
                    .await
                    .unwrap()
                    .unwrap()
                    .supplier_account_id
                    .as_deref(),
                Some(ids[0].as_str())
            );
        } else if available {
            assert_eq!(value["type"], "response.created");
            assert_eq!(value["response"]["id"], "winning-attempt");
            let event = tokio::time::timeout(std::time::Duration::from_secs(10), client.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let value: Value = serde_json::from_slice(&event.into_data()).unwrap();
            assert_eq!(value["type"], "response.completed");
            // Receiving completion proves the ledger was persisted before delivery.
            let rows: Vec<(String, String, Option<i64>)> =
                sqlx::query_as("SELECT account_id,status,input_tokens FROM usage_records")
                    .fetch_all(state.storage.pool())
                    .await
                    .unwrap();
            assert_eq!(rows, vec![(ids[1].clone(), "completed".into(), Some(7))]);
            client.close(None).await.unwrap();
        } else {
            assert_eq!(value["status"], 503);
            assert_eq!(value["error"]["code"], "supplier_pool_exhausted");
            assert!(!value.to_string().contains("rejected-attempt"));
            let close = client.next().await.unwrap().unwrap();
            assert!(
                matches!(close,UpstreamMessage::Close(Some(frame)) if u16::from(frame.code)==1011)
            );
            let rows: Vec<(String, String)> =
                sqlx::query_as("SELECT status,error_code FROM usage_records")
                    .fetch_all(state.storage.pool())
                    .await
                    .unwrap();
            assert_eq!(
                rows,
                vec![("failed".into(), "supplier_pool_exhausted".into())]
            );
        }
        let admissions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM virtual_request_admissions")
            .fetch_one(state.storage.pool())
            .await
            .unwrap();
        assert_eq!(admissions, 1);
        upstream_task.await.unwrap();
        if let Some(task) = second_task {
            if throttle {
                task.abort();
            } else {
                task.await.unwrap();
            }
        }
        app_task.abort();
    }
}

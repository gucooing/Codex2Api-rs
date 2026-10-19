//! Native Codex Responses WebSocket sessions. Each generation uses the same
//! API admission, supplier scheduler and usage ledger as the HTTP transport.
use crate::{
    ApiCredential, ApiError, ApiState, InboundRequest, PreparedRequest, Protocol, Result, dispatch,
    execution, response, runtime::ApiPermit, usage::ApiLog,
};
use axum::{
    extract::{
        WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    response::Response,
};
use codex2api_upstream::protocol::responses::Output as ResponsesOutput;
use codex2api_upstream::scheduler::{ScheduleRequest, SupplierContext, SupplierRoute};
use codex2api_upstream::{
    Endpoint, ResponseOutcome, SupplierFailure, UpstreamError, UpstreamWebSocket,
};
use futures::{SinkExt, StreamExt};
use http::{HeaderMap, StatusCode};
use serde_json::Value;
use std::collections::{HashSet, VecDeque};
use tokio_tungstenite::tungstenite::Message as UpstreamMessage;

struct Session {
    state: ApiState,
    credential: ApiCredential,
    route: SupplierRoute,
    headers: HeaderMap,
    user_agent: Option<String>,
    client: Option<std::net::IpAddr>,
}

struct Connection {
    socket: UpstreamWebSocket,
    context: SupplierContext,
    headers: HeaderMap,
    revision: Option<i64>,
    workspace: Option<codex2api_upstream::WorkspaceConnection>,
    timezone: Option<String>,
}

impl Session {
    async fn open(
        &self,
        headers: &HeaderMap,
        request: &ScheduleRequest,
        excluded: &mut Vec<String>,
        mut selected: Option<SupplierContext>,
    ) -> Result<Connection> {
        loop {
            dispatch::validate(
                &self.state,
                &self.credential.principal,
                &self.credential.token,
            )
            .await?;
            let context = match selected.take() {
                Some(context) => context,
                None => {
                    codex2api_upstream::scheduler::select(
                        &self.state.storage,
                        &self.route,
                        request,
                        excluded,
                        || {
                            dispatch::validate(
                                &self.state,
                                &self.credential.principal,
                                &self.credential.token,
                            )
                        },
                    )
                    .await?
                }
            };
            let revision = self
                .state
                .storage
                .supplier_auth_revision(&context.account.id)
                .await?;
            let attempt = async {
                let client = self.state.upstream.get(&context.account.id).await?;
                let timezone = client.identity().http_fingerprint.timezone.clone();
                let headers =
                    dispatch::owned_headers(&self.state, &self.route, &context, headers).await?;
                let (socket, headers, workspace) = client
                    .connect_websocket(Endpoint::Responses, headers)
                    .await?;
                Ok::<_, UpstreamError>((socket, headers, workspace, timezone))
            }
            .await;
            match attempt {
                Ok((socket, headers, workspace, timezone)) => {
                    let revision = workspace
                        .as_ref()
                        .map(|workspace| workspace.auth_revision())
                        .or(revision);
                    dispatch::remember_affinity(
                        &self.state,
                        &self.route,
                        &context.account.id,
                        None,
                        Some(&headers),
                    )
                    .await?;
                    return Ok(Connection {
                        socket,
                        context,
                        headers,
                        revision,
                        workspace,
                        timezone,
                    });
                }
                Err(error) => {
                    let Some(failure) = error.supplier_failure(chrono::Utc::now().timestamp())
                    else {
                        return Err(error.into());
                    };
                    // The native connector already refreshes an expired credential
                    // once. Only unrecoverable authorization, quota or billing
                    // rejection can select another account.
                    codex2api_upstream::scheduler::observe_failure(
                        &self.state.storage,
                        &context.account.id,
                        error.auth_revision().or(revision),
                        &failure,
                    )
                    .await?;
                    excluded.push(context.account.id.clone());
                }
            }
        }
    }
}

impl Connection {
    async fn create(&mut self, prepared: &PreparedRequest) -> Result<()> {
        if let Some(workspace) = &self.workspace {
            workspace.check_current().await?;
        }
        let mut value = prepared.body.clone();
        value["type"] = "response.create".into();
        codex2api_upstream::apply_response_timezone(&mut value, self.timezone.as_deref())?;
        codex2api_upstream::normalize_response_identity(
            &mut value,
            &self.context.account.installation_id,
            &prepared.headers,
        )?;
        let text = codex2api_upstream::serialize_responses_request(&value)?;
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.socket.send(UpstreamMessage::Text(text.into())),
        )
        .await
        .map_err(|_| ApiError::from(UpstreamError::StreamIdleTimeout))?
        .map_err(|error| ApiError::from(UpstreamError::Stream(error.to_string())))
    }
}

pub(crate) async fn connect(
    state: ApiState,
    credential: ApiCredential,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    dispatch::validate(&state, &credential.principal, &credential.token).await?;
    let user_agent = codex2api_upstream::request_user_agent(&headers);
    let headers =
        codex2api_upstream::protocol::chatgpt_context(&mut serde_json::json!({}), &headers)?;
    let session = Session {
        route: dispatch::route(&credential.principal, Some(codex2api_common::CHATGPT)),
        state,
        credential,
        headers,
        user_agent,
        client: codex2api_upstream::RequestClock::current().and_then(|clock| clock.client),
    };
    let request = codex2api_upstream::protocol::scheduling(
        Some(codex2api_common::CHATGPT),
        &session.headers,
        &serde_json::json!({}),
    )?;
    let mut connection = session
        .open(&session.headers, &request, &mut Vec::new(), None)
        .await?;
    let mut response_headers = response::public_headers(&connection.headers, false);
    response_headers.remove("content-type");
    // Idle reusable connections do not consume a generation concurrency slot.
    connection.context.permit.take();
    let mut response = upgrade
        .max_message_size(codex2api_upstream::MAX_REQUEST_BYTES)
        .max_frame_size(codex2api_upstream::MAX_REQUEST_BYTES)
        .on_upgrade(move |socket| bridge(session, connection, socket));
    response.headers_mut().extend(response_headers);
    Ok(response)
}

struct Turn {
    prepared: PreparedRequest,
    log: Option<ApiLog>,
    _permit: Option<ApiPermit>,
    output: ResponsesOutput,
    response_id: Option<String>,
    prelude: Vec<Value>,
    prelude_bytes: usize,
    committed: bool,
    excluded: Vec<String>,
    refreshed: HashSet<String>,
    deadline: tokio::time::Instant,
}

impl Turn {
    fn supplier(&mut self, connection: &Connection) {
        if let Some(log) = &mut self.log {
            log.record.account_id = connection.context.account.id.clone();
            log.record.upstream_endpoint = Some(Protocol::Responses.endpoint().into());
            log.record.account_name = connection
                .context
                .account
                .display_name
                .clone()
                .unwrap_or_else(|| connection.context.account.email.clone().unwrap_or_default());
            log.headers(StatusCode::SWITCHING_PROTOCOLS, &connection.headers);
            log.auth_revision = connection.revision;
        }
    }

    async fn deliver(
        &mut self,
        session: &Session,
        account_id: &str,
        client: &mut WebSocket,
        connected: &mut bool,
        mut value: Value,
    ) -> Result<Option<ResponseOutcome>> {
        self.output.observe(&mut value)?;
        let outcome = ResponseOutcome::from_value(&value, None);
        if let Some(id) = value
            .pointer("/response/id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
        {
            if self.response_id.as_deref() != Some(id) {
                dispatch::remember_affinity(
                    &session.state,
                    &session.route,
                    account_id,
                    Some(id),
                    None,
                )
                .await?;
            }
            self.response_id = Some(id.to_owned());
        }
        if let Some(log) = &mut self.log {
            log.observe(&value, Protocol::Responses);
            if outcome.is_some() {
                log.settle().await?;
            }
        }
        // Ordinary payload is kept intact; only supplier envelopes are scrubbed.
        codex2api_upstream::public_output::metadata(&mut value);
        if let Some(metadata) = &self.prepared.metadata
            && let Some(response) = value.get_mut("response").filter(|v| v.is_object())
        {
            response["metadata"] = metadata.clone();
        }
        if *connected
            && !matches!(
                tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    client.send(Message::Text(value.to_string().into()))
                )
                .await,
                Ok(Ok(()))
            )
        {
            *connected = false;
        }
        Ok(outcome)
    }
}

async fn bridge(session: Session, mut connection: Connection, mut client: WebSocket) {
    let trace = codex2api_upstream::request_debug::RequestTrace::new("websocket", "/v1/responses");
    let mut queue = VecDeque::<(PreparedRequest, usize, codex2api_upstream::RequestClock)>::new();
    let mut queued_bytes = 0usize;
    let mut pending: Option<Turn> = None;
    let mut connected = true;
    let mut close_code = 1000;
    let mut last_upstream = tokio::time::Instant::now();
    let result: Result<()> = async {
        loop {
            if !connected {
                queue.clear();
                queued_bytes = 0;
                if pending.is_none() { return Ok(()); }
            }
            if pending.is_none() && let Some((prepared, bytes, clock)) = queue.pop_front() {
                queued_bytes -= bytes;
                dispatch::validate(&session.state, &session.credential.principal, &session.credential.token).await?;
                let warmup = prepared.body["generate"] == false;
                let (log, permit) = if warmup {
                    let model = prepared.body["model"].as_str().unwrap_or("");
                    let models = session.state.storage.api_group_models(&session.credential.principal.group.id).await?;
                    if !models.iter().any(|item| item.model == model) {
                        return Err(ApiError::new(StatusCode::FORBIDDEN, "api_model_unavailable",
                            "The model is not available in this API group."));
                    }
                    (None, None)
                } else {
                    let (log, permit) = clock.scope(execution::admit(&session.state, &session.credential.principal, &prepared.body, prepared.inbound.endpoint(), true, "websocket", session.user_agent.as_deref())).await?;
                    (Some(log), Some(permit))
                };
                // Store admission before fallible scheduling/connection work so
                // any rejection still settles the shared request row.
                pending = Some(Turn { prepared, log, _permit: permit, output: Default::default(),
                    response_id: None, prelude: Vec::new(), prelude_bytes: 0, committed: false,
                    excluded: Vec::new(), refreshed: HashSet::new(),
                    deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(3600) });
                let turn = pending.as_mut().unwrap();
                let schedule = codex2api_upstream::protocol::scheduling(Some(codex2api_common::CHATGPT), &session.headers, &turn.prepared.body)?;
                let selected = codex2api_upstream::scheduler::select(&session.state.storage, &session.route, &schedule, &[],
                    || dispatch::validate(&session.state, &session.credential.principal, &session.credential.token)).await?;
                let revision = session.state.storage.supplier_auth_revision(&selected.account.id).await?;
                if selected.account.id != connection.context.account.id || revision != connection.revision {
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), connection.socket.close(None)).await;
                    connection = session.open(&turn.prepared.headers, &schedule, &mut turn.excluded, Some(selected)).await?;
                } else {
                    connection.context = selected;
                }
                turn.supplier(&connection);
                if let Some(log)=&turn.log {session.state.storage.set_usage_upstream_endpoint(&log.record.id,Protocol::Responses.endpoint()).await?;}
                connection.create(&turn.prepared).await?;
                last_upstream = tokio::time::Instant::now();
            }
            let idle = last_upstream + codex2api_upstream::DEFAULT_STREAM_IDLE_TIMEOUT;
            let deadline = pending.as_ref().map_or(idle, |turn| idle.min(turn.deadline));
            tokio::select! {
                incoming = client.next(), if connected => {
                    let clock = codex2api_upstream::RequestClock::arrival(session.client);
                    let Some(Ok(message)) = incoming else { connected = false; continue };
                    if let Message::Text(value) = &message { trace.input(value.as_bytes()); }
                    if let Message::Binary(value) = &message { trace.input(value); }
                    let parsed: Result<()> = async {
                        let text = match message {
                            Message::Text(value) => value.to_string(),
                            Message::Binary(value) => String::from_utf8(value.to_vec())
                                .map_err(|_| ApiError::bad_request("Responses frames must contain UTF-8 JSON."))?,
                            Message::Close(_) => { connected = false; return Ok(()); }
                            Message::Ping(_) | Message::Pong(_) => return Ok(()),
                        };
                        dispatch::validate(&session.state, &session.credential.principal, &session.credential.token).await?;
                        let mut value: Value = serde_json::from_str(&text)
                            .map_err(|_| ApiError::bad_request("Invalid Responses WebSocket JSON."))?;
                        match value["type"].as_str() {
                            Some("response.create") => {
                                if queue.len() >= 32 || queued_bytes.saturating_add(text.len()) > codex2api_upstream::MAX_REQUEST_BYTES {
                                    return Err(ApiError::bad_request("Too many queued Responses requests."));
                                }
                                crate::boolean(&value, "generate", true)?;
                                if value.get("stream").is_some_and(|value| value != true) {
                                    return Err(ApiError::bad_request("Responses WebSocket requires stream=true."));
                                }
                                value.as_object_mut().unwrap().remove("type");
                                value["stream"] = true.into();
                                let input = InboundRequest::new(Protocol::Responses, value, session.headers.clone())?;
                                let prepared = codex2api_upstream::protocol::prepare_for(codex2api_common::CHATGPT, &input)?;
                                queued_bytes += text.len();
                                queue.push_back((prepared, text.len(), clock));
                            }
                            Some("response.interrupt") => {
                                let id = value["response_id"].as_str().filter(|id| !id.is_empty());
                                if id.is_none() || pending.as_ref().and_then(|turn| turn.response_id.as_deref()) != id {
                                    return Err(ApiError::bad_request("response_id must identify the active response."));
                                }
                                if value["mode"] != "discard_partial_items" {
                                    return Err(ApiError::bad_request("response.interrupt requires mode=discard_partial_items."));
                                }
                                let control = serde_json::json!({"type":"response.interrupt","response_id":id,"mode":"discard_partial_items"});
                                tokio::time::timeout(std::time::Duration::from_secs(30),
                                    connection.socket.send(UpstreamMessage::Text(control.to_string().into()))).await
                                    .map_err(|_| ApiError::from(UpstreamError::StreamIdleTimeout))?
                                    .map_err(|error| ApiError::from(UpstreamError::Stream(error.to_string())))?;
                            }
                            _ => return Err(ApiError::bad_request("Unsupported Responses WebSocket event type.")),
                        }
                        Ok(())
                    }.await;
                    if let Err(error) = parsed {
                        reject(&mut client, &error, &trace).await;
                        connected = false;
                        // Continue receiving an already accepted generation so
                        // bad client input cannot erase its real billable usage.
                    }
                }
                incoming = connection.socket.next() => {
                    if let Some(log) = pending.as_mut().and_then(|turn| turn.log.as_mut()) {
                        log.set_timing(connection.socket.generation_timing());
                    }
                    last_upstream = tokio::time::Instant::now();
                    let message = match incoming {
                        Some(Ok(message)) => message,
                        Some(Err(error)) => return Err(UpstreamError::Stream(error.to_string()).into()),
                        None if pending.is_none() => return Ok(()),
                        None => return Err(ApiError::upstream_incomplete()),
                    };
                    let text = match message {
                        UpstreamMessage::Text(value) => value.to_string(),
                        UpstreamMessage::Binary(value) => String::from_utf8(value.to_vec())
                            .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "upstream_protocol_error", "The upstream frame was not UTF-8."))?,
                        UpstreamMessage::Close(_) if pending.is_none() => return Ok(()),
                        UpstreamMessage::Close(_) => return Err(ApiError::upstream_incomplete()),
                        _ => continue,
                    };
                    trace.output(text.as_bytes());
                    if text.len() > codex2api_upstream::MAX_REQUEST_BYTES {
                        return Err(ApiError::new(StatusCode::BAD_GATEWAY, "upstream_response_too_large", "The upstream frame exceeded the size limit."));
                    }
                    let value: Value = serde_json::from_str(&text)
                        .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "upstream_protocol_error", "The upstream frame was not valid JSON."))?;
                    if !value.is_object() {
                        return Err(ApiError::new(StatusCode::BAD_GATEWAY, "upstream_protocol_error", "The upstream frame was not a Responses event."));
                    }
                    if matches!(value["type"].as_str(), Some("codex.rate_limits" | "rate_limits.updated")) {
                        continue;
                    }
                    let Some(turn) = pending.as_mut() else {
                        if ResponseOutcome::from_value(&value, None).is_some_and(|outcome| matches!(outcome, ResponseOutcome::Failed(_))) {
                            return Err(ApiError::upstream_value(&value));
                        }
                        continue;
                    };
                    if let Some(mut failure) = codex2api_upstream::classify_supplier_failure(
                        None, &value, &connection.headers, chrono::Utc::now().timestamp())
                    {
                        let replay_safe = !turn.committed && !codex2api_upstream::scheduler::generation_observed(&value);
                        let recovered = if replay_safe && failure == SupplierFailure::Authentication
                            && turn.refreshed.insert(connection.context.account.id.clone())
                        {
                            match codex2api_upstream::scheduler::recover_auth(&session.state.storage, &session.state.upstream,
                                &connection.context, connection.revision).await?
                            {
                                None => true,
                                Some(rejection) => { failure = rejection; false }
                            }
                        } else { false };
                        if !recovered {
                            codex2api_upstream::scheduler::observe_failure(&session.state.storage, &connection.context.account.id,
                                connection.revision, &failure).await?;
                        }
                        if replay_safe {
                            if !recovered { turn.excluded.push(connection.context.account.id.clone()); }
                            connection.context.permit.take();
                            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), connection.socket.close(None)).await;
                            let schedule = codex2api_upstream::protocol::scheduling(Some(codex2api_common::CHATGPT), &session.headers, &turn.prepared.body)?;
                            connection = session.open(&turn.prepared.headers, &schedule, &mut turn.excluded, None).await?;
                            turn.prelude.clear();
                            turn.prelude_bytes = 0;
                            turn.response_id = None;
                            turn.supplier(&connection);
                if let Some(log)=&turn.log {session.state.storage.set_usage_upstream_endpoint(&log.record.id,Protocol::Responses.endpoint()).await?;}
                            connection.create(&turn.prepared).await?;
                            continue;
                        }
                    }
                    if !turn.committed && codex2api_upstream::scheduler::prelude_event(&value)
                        && turn.prelude_bytes.saturating_add(text.len()) <= 256 * 1024
                    {
                        // The client may interrupt after response.created. The
                        // prelude becomes visible once upstream execution begins.
                        turn.prelude_bytes += text.len();
                        turn.prelude.push(value);
                        continue;
                    }
                    for event in std::mem::take(&mut turn.prelude) {
                        turn.deliver(&session, &connection.context.account.id, &mut client, &mut connected, event).await?;
                    }
                    turn.committed = true;
                    let outcome = turn.deliver(&session, &connection.context.account.id, &mut client, &mut connected, value).await?;
                    if let Some(outcome) = outcome {
                        pending.take();
                        connection.context.permit.take();
                        if let ResponseOutcome::Failed(failure) = outcome {
                            close_code = if failure.status.is_some_and(|status| status < 500) { 1008 } else { 1011 };
                            return Ok(());
                        }
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    if pending.is_none() { return Ok(()); }
                    return Err(UpstreamError::StreamIdleTimeout.into());
                }
            }
        }
    }.await;
    if let Err(error) = result {
        if let Some(log) = pending.as_mut().and_then(|turn| turn.log.as_mut()) {
            log.fail(&error);
            if let Err(error) = log.settle().await {
                tracing::error!(%error, "Could not settle API WebSocket request");
            }
        }
        close_code = if error.status.is_client_error() {
            1008
        } else {
            1011
        };
        if connected {
            reject(&mut client, &error, &trace).await;
            connected = false;
        }
    }
    if connected {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.send(Message::Close(Some(CloseFrame {
                code: close_code,
                reason: "Responses connection ended".into(),
            }))),
        )
        .await;
    }
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        connection.socket.close(None),
    )
    .await;
}

async fn reject(
    client: &mut WebSocket,
    error: &ApiError,
    trace: &codex2api_upstream::request_debug::RequestTrace,
) {
    let message = response::error_event(error).to_string();
    trace.output(message.as_bytes());
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        client.send(Message::Text(message.into())),
    )
    .await;
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        client.send(Message::Close(Some(CloseFrame {
            code: if error.status.is_client_error() {
                1008
            } else {
                1011
            },
            reason: "Responses request failed".into(),
        }))),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex2api_storage::{
        ApiGroup, ModelConfig, ModelPrice, NewSupplierAccount, SupplierTokens, User, UserKind,
        WalletAdjustment,
    };
    use serde_json::json;

    #[tokio::test]
    async fn api_rewrite_native_socket_reuses_two_generations_settles_before_delivery_and_closes_on_error()
     {
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            let directory = codex2api_storage::test_support::tempdir().unwrap();
            let storage = codex2api_storage::test_support::open(directory.path().join("native-ws-pg")).await.unwrap();
            storage.save_user(&User {
                id: "consumer".into(), username: "consumer".into(), password_hash: "fixture".into(),
                name: "Consumer".into(), email: "consumer@example.test".into(), enabled: true, wallet_cents: 0,
                revision: 1, created_at: chrono::Utc::now().to_rfc3339(), kind: UserKind::Regular,
            }, None).await.unwrap();
            storage.adjust_user_wallet("consumer", storage.require_admin_user().await.unwrap().id,
                WalletAdjustment { request_id: "fund".into(), amount_cents: 100, expected_revision: 1, reason: None }
            ).await.unwrap();
            storage.save_model_config(&ModelConfig {
                model: "custom-model".into(), platform: "chatgpt".into(), kind: "text".into(),
                enabled: true, deleted: false, revision: 0,
            }, &[ModelPrice {
                model: "custom-model".into(), tier: "standard".into(),
                min_input_tokens: 0, max_input_tokens: None, input_rate: 1_000_000,
                cached_rate: 100_000, cache_write_rate: 1_000_000, output_rate: 2_000_000,
                source: "custom".into(), revision: 0,
            }], &[], None).await.unwrap();
            let identity = codex2api_suppliers::AccountIdentity::new(
                "supplier", uuid::Uuid::new_v4().to_string(), codex2api_suppliers::HostRuntime {
                    terminal: "fixture".into(),
                    os_type: "Linux".into(), os_version: "6".into(), arch: "x86_64".into(),
                });
            let mut supplier = NewSupplierAccount::pending_identity(
                &identity.installation_id,
                &identity.os_type, &identity.os_version, &identity.arch, identity.fingerprint_json().unwrap(),
            );
            supplier.id = Some("supplier".into());
            supplier.chatgpt_account_id = Some("supplier-workspace".into());
            supplier.chatgpt_user_id = Some("supplier-user".into());
            storage.save_authorized_account(supplier, SupplierTokens {
                access_token: Some("fixture-token".into()), auth_mode: Some("chatgpt".into()),
                last_refresh: Some(chrono::Utc::now().to_rfc3339()), ..Default::default()
            }, None).await.unwrap();
            storage.add_supplier_model("supplier", "custom-model").await.unwrap();
            storage.save_supplier_tag("pool", "chatgpt", "Pool").await.unwrap();
            storage.edit_supplier_tags(&["supplier".into()], &["pool".into()], false).await.unwrap();
            storage.save_api_group(&ApiGroup {
                id: "group".into(), name: "Group".into(), description: String::new(),
                enabled: true, tag_ids: vec!["pool".into()],
                models: vec![codex2api_common::ModelRule::granted("custom-model")],
                multiplier: "1".into(), concurrency_limit: 1,
                rpm_limit: 0, revision: 0, search_price_usd: None,
            }, None).await.unwrap();
            let key = storage.user_store().create_api_key("consumer", "Key", "group").await.unwrap();
            let principal = storage.authenticate_api_key(&key.secret).await.unwrap().unwrap();
            let auth = codex2api_suppliers::AuthService::new(
                codex2api_suppliers::SupplierAccountStore::open(storage.clone())).unwrap();
            let state = ApiState::new(storage.clone(), codex2api_upstream::UpstreamPool::new(auth));

            let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let upstream_address = upstream_listener.local_addr().unwrap();
            let installation = identity.installation_id;
            let upstream = tokio::spawn(async move {
                let (tcp, _) = upstream_listener.accept().await.unwrap();
                let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
                for turn in 0..3 {
                    let message = socket.next().await.unwrap().unwrap();
                    let value: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
                    assert_eq!(value["type"], "response.create");
                    assert_eq!(value["client_metadata"]["session_id"], "caller-session");
                    assert_eq!(value["client_metadata"]["x-codex-installation-id"], installation);
                    assert_eq!(value["prompt_cache_key"], "caller-cache");
                    assert_eq!(value["generate"] == false, turn == 0);
                    if turn == 2 { assert_eq!(value["previous_response_id"], "resp-1"); }
                    let id = format!("resp-{turn}");
                    let item = json!({"type":"message","id":format!("msg-{turn}"),"role":"assistant",
                        "content":[{"type":"output_text","text":"actual output","annotations":[]} ]});
                    socket.send(UpstreamMessage::Text(json!({"type":"response.created",
                        "response":{"id":id,"status":"in_progress","model":"custom-model"}}).to_string().into())).await.unwrap();
                    if turn > 0 {
                        socket.send(UpstreamMessage::Text(json!({"type":"response.output_item.done",
                            "output_index":0,"item":item}).to_string().into())).await.unwrap();
                    }
                    let mut response = json!({"id":id,"status":"completed","output":[]});
                    if turn > 0 {
                        response["usage"] = json!({"input_tokens":10000,"output_tokens":5000,
                            "input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0}});
                    }
                    socket.send(UpstreamMessage::Text(json!({"type":"response.completed","response":response})
                        .to_string().into())).await.unwrap();
                }
                assert!(matches!(socket.next().await, Some(Ok(UpstreamMessage::Close(_)))));
            });
            let tcp = tokio::net::TcpStream::connect(upstream_address).await.unwrap();
            let socket = tokio_tungstenite::client_async_tls_with_config(
                format!("ws://{upstream_address}/responses"),
                tokio_tungstenite::MaybeTlsStream::Plain(tcp), None, None,
            ).await.unwrap().0.into();
            let connection = Connection {
                socket, context: SupplierContext {
                    account: storage.supplier_account_snapshot("supplier").await.unwrap(),
                    reused_affinity: false, previous_response_matches: false,
                    permit: None, request: ScheduleRequest::default(),
                }, headers: HeaderMap::new(),
                revision: storage.supplier_auth_revision("supplier").await.unwrap(),
                workspace: None, timezone: None,
            };
            let mut inbound = HeaderMap::new();
            inbound.insert("session-id", "caller-session".parse().unwrap());
            inbound.insert("user-agent", "consumer-websocket/1".parse().unwrap());
            inbound.insert("openai-project", "untrusted-project".parse().unwrap());
            inbound.insert("openai-organization", "untrusted-organization".parse().unwrap());
            inbound.insert("chatgpt-account-id", "untrusted-account".parse().unwrap());
            let headers = codex2api_upstream::protocol::chatgpt_context(&mut json!({}), &inbound).unwrap();
            assert_eq!(headers.len(), 1);
            assert_eq!(headers["session-id"], "caller-session");
            let session = Session {
                route: dispatch::route(&principal, Some(codex2api_common::CHATGPT)), state: state.clone(),
                credential: ApiCredential { principal, token: key.secret }, headers,
                user_agent: codex2api_upstream::request_user_agent(&inbound),
                client: None,
            };
            let ready = std::sync::Arc::new(tokio::sync::Mutex::new(Some((session, connection))));
            let router = axum::Router::new().route("/responses", axum::routing::get(move |upgrade: WebSocketUpgrade| {
                let ready = ready.clone();
                async move {
                    let (session, connection) = ready.lock().await.take().unwrap();
                    upgrade.on_upgrade(move |client| bridge(session, connection, client))
                }
            }));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{address}/responses")).await.unwrap();
            for turn in 0..3 {
                let mut request = json!({"type":"response.create","model":"custom-model","stream":true,
                    "input":"Hello","prompt_cache_key":"caller-cache","metadata":{"account":"caller value"}});
                if turn == 0 { request["generate"] = false.into(); }
                if turn == 2 { request["previous_response_id"] = "resp-1".into(); }
                client.send(UpstreamMessage::Text(request.to_string().into())).await.unwrap();
                loop {
                    let message = client.next().await.unwrap().unwrap();
                    let value: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
                    if value["type"] != "response.completed" { continue; }
                    assert_eq!(value["response"]["metadata"]["account"], "caller value");
                    if turn > 0 { assert_eq!(value["response"]["output"][0]["content"][0]["text"], "actual output"); }
                    // Terminal delivery is the boundary: all preceding generations
                    // have already committed usage and their wallet ledger.
                    let rows: Vec<(String, Option<i64>, Option<i64>, String)> = sqlx::query_as(
                        "SELECT status,input_tokens,output_tokens,transport FROM usage_records ORDER BY requested_at_ms")
                        .fetch_all(storage.pool()).await.unwrap();
                    assert_eq!(rows.len(), turn);
                    let timings: Vec<Option<i64>> = sqlx::query_scalar("SELECT first_token_ms FROM usage_records")
                        .fetch_all(storage.pool()).await.unwrap();
                    assert!(timings.iter().all(Option::is_some));
                    let agents: Vec<Option<String>> = sqlx::query_scalar("SELECT user_agent FROM usage_records")
                        .fetch_all(storage.pool()).await.unwrap();
                    assert!(agents.iter().all(|agent| agent.as_deref() == Some("consumer-websocket/1")));
                    assert!(rows.iter().all(|(status,input,output,transport)|
                        status == "completed" && *input == Some(10000) && *output == Some(5000) && transport == "websocket"));
                    let debits: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM usage_records WHERE wallet_debit_cents>0").fetch_one(storage.pool()).await.unwrap();
                    assert_eq!(debits as usize, turn);
                    break;
                }
            }
            client.send(UpstreamMessage::Text(json!({"type":"response.cancel"}).to_string().into())).await.unwrap();
            let error = client.next().await.unwrap().unwrap();
            let error: Value = serde_json::from_str(error.to_text().unwrap()).unwrap();
            assert_eq!(error["type"], "error");
            assert_eq!(error["status"], 400);
            assert!(matches!(client.next().await, Some(Ok(UpstreamMessage::Close(Some(frame)))) if frame.code == 1008.into()));
            upstream.await.unwrap();
            assert_eq!(state.runtime().current_for_key(&key.key.id), 0);
            server.abort();
        }).await.unwrap();
    }
}

use crate::{
    GatewayError, GatewayState, OutputMode, PreparedRequest, Result, providers,
    runtime::{ApiPermit, Limits},
    usage::ApiLog,
};
use axum::{
    Json,
    body::{Body, Bytes},
    response::{IntoResponse, Response},
};
use codex2api_scheduler::{SupplierContext, SupplierRoute};
use codex2api_storage::{ApiPrincipal, UsageRecord};
use codex2api_upstream::{ResponseOutcome, SupplierFailure, UpstreamError};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use http::{HeaderMap, StatusCode};
use serde_json::Value;
use std::time::Instant;

pub(crate) async fn execute_prepared(
    state: GatewayState,
    principal: ApiPrincipal,
    token: String,
    headers: HeaderMap,
    prepared: PreparedRequest,
) -> Result<Response> {
    // The accepted request and its usage continue if the downstream disconnects.
    tokio::spawn(run(state, principal, token, headers, prepared))
        .await
        .map_err(|error| {
            tracing::error!(%error,"API request worker failed");
            GatewayError::internal()
        })?
}

async fn run(
    state: GatewayState,
    principal: ApiPrincipal,
    token: String,
    headers: HeaderMap,
    prepared: PreparedRequest,
) -> Result<Response> {
    let metadata = codex2api_upstream::request_metadata_value(&prepared.body);
    let model = metadata
        .model
        .as_deref()
        .filter(|model| codex2api_core::valid_model(model))
        .ok_or_else(|| GatewayError::bad_request("model must name a configured model"))?;
    let rule = principal
        .group
        .model_rules
        .iter()
        .find(|rule| rule.model == model);
    let limits = Limits {
        group_concurrency: principal.group.concurrency_limit,
        group_rpm: principal.group.rpm_limit,
        model_concurrency: rule.map_or(0, |rule| rule.concurrency_limit),
        model_rpm: rule.map_or(0, |rule| rule.rpm_limit),
    };
    let model = model.to_owned();
    let started = Instant::now();
    let record = UsageRecord {
        id: uuid::Uuid::new_v4().to_string(),
        provider_id: principal.group.provider_id.clone(),
        subject_id: principal.user_id.clone(),
        subject_kind: "api_key".into(),
        endpoint: prepared.endpoint.into(),
        transport: "http".into(),
        model: metadata.model,
        reasoning_effort: metadata.reasoning_effort,
        service_tier: metadata.service_tier,
        image_size: metadata.image_size,
        image_input_usage_json: metadata.image_input_sizes.map(|sizes| {
            serde_json::to_string(
                &sizes
                    .into_iter()
                    .map(|resolution| codex2api_storage::ImageUsage {
                        resolution: Some(resolution),
                        count: 1,
                    })
                    .collect::<Vec<_>>(),
            )
            .expect("image usage")
        }),
        requested_at_ms: chrono::Utc::now().timestamp_millis(),
        status: "in_progress".into(),
        ..Default::default()
    };
    state.storage.start_api_request(&principal, &record).await?;
    let mut log = ApiLog::new(state.storage.clone(), record, started);
    let permit = match state.runtime.acquire(
        &principal.user_id,
        &principal.key_id,
        &principal.group.id,
        &model,
        limits,
    ) {
        Ok(permit) => permit,
        Err(error) => {
            let error = GatewayError::from(error);
            log.fail(&error);
            log.settle().await?;
            return Err(error);
        }
    };
    let result = forward(&state, &principal, &token, &headers, &prepared, &mut log).await;
    match result {
        Ok((response, context, route)) => {
            response_body(state, log, permit, context, route, response, prepared).await
        }
        Err(error) => {
            log.fail(&error);
            log.settle().await?;
            Err(error)
        }
    }
}

async fn validate(
    state: &GatewayState,
    principal: &ApiPrincipal,
    token: &str,
) -> codex2api_scheduler::Result<()> {
    let current = state
        .storage
        .authenticate_api_key(token)
        .await?
        .ok_or(codex2api_scheduler::SchedulerError::ConsumerInvalid)?;
    if current.user_id != principal.user_id
        || current.key_id != principal.key_id
        || current.group.id != principal.group.id
        || current.group.revision != principal.group.revision
    {
        return Err(codex2api_scheduler::SchedulerError::ConsumerInvalid);
    }
    Ok(())
}

async fn forward(
    state: &GatewayState,
    principal: &ApiPrincipal,
    token: &str,
    headers: &HeaderMap,
    prepared: &PreparedRequest,
    log: &mut ApiLog,
) -> Result<(reqwest::Response, SupplierContext, SupplierRoute)> {
    let route = SupplierRoute {
        owner_id: format!("api:{}:{}", principal.user_id, principal.group.id),
        provider_id: principal.group.provider_id.clone(),
        tag_ids: principal.group.tag_ids.clone(),
    };
    let schedule = match route.provider_id.as_str() {
        codex2api_core::CHATGPT => providers::chatgpt::scheduling(headers, &prepared.body),
        codex2api_core::GROK => providers::grok::scheduling(headers, &prepared.body),
        _ => return Err(GatewayError::bad_request("Unsupported API platform")),
    };
    let outbound = match route.provider_id.as_str() {
        codex2api_core::CHATGPT => {
            providers::chatgpt::outbound_headers(headers, prepared.output == OutputMode::Images)
        }
        codex2api_core::GROK => providers::grok::outbound_headers(),
        _ => HeaderMap::new(),
    };
    let mut excluded = vec![];
    let mut refreshed = std::collections::HashSet::new();
    loop {
        validate(state, principal, token).await?;
        let context =
            codex2api_scheduler::select(&state.storage, &route, &schedule, &excluded, || {
                validate(state, principal, token)
            })
            .await?;
        log.record.account_id = context.account.id.clone();
        log.record.account_name = context.account.display_name.clone().unwrap_or_default();
        // A refresh retries this supplier once without releasing its reserved slot.
        loop {
            let mut revision = state
                .storage
                .supplier_auth_revision(&context.account.id)
                .await?;
            log.record.http_status = None;
            let result = async {
                let response = send(state, &context, &outbound, prepared).await?;
                revision = response
                    .extensions()
                    .get::<codex2api_upstream::SupplierAuthRevision>()
                    .map(|r| r.0)
                    .or(revision);
                let prelude = if route.provider_id == codex2api_core::GROK {
                    providers::grok::prelude
                } else {
                    codex2api_scheduler::prelude_event
                };
                codex2api_scheduler::inspect_response(&route.provider_id, prelude, response).await
            }
            .await;
            match result {
                Ok(response) => return Ok((response, context, route)),
                Err(error) => {
                    if let UpstreamError::Status {
                        status,
                        body,
                        headers,
                        ..
                    } = &error
                    {
                        log.record.http_status = Some(i64::from(*status));
                        if let Ok(value) = serde_json::from_str::<Value>(body) {
                            if codex2api_scheduler::generation_observed(&value) {
                                log.headers(
                                    StatusCode::from_u16(*status)
                                        .unwrap_or(StatusCode::BAD_GATEWAY),
                                    headers,
                                );
                                log.measure(&value, prepared.output);
                                if let Some(failure) = error.supplier_failure_for(
                                    &route.provider_id,
                                    chrono::Utc::now().timestamp(),
                                ) {
                                    codex2api_scheduler::observe_failure(
                                        &state.storage,
                                        &context.account.id,
                                        error.auth_revision().or(revision),
                                        &failure,
                                    )
                                    .await?;
                                }
                                // Output or usage proves this attempt ran. Never duplicate it on
                                // another supplier or refresh and execute the request again.
                                return Err(error.into());
                            }
                        }
                    }
                    revision = error.auth_revision().or(revision);
                    let Some(mut failure) = error
                        .supplier_failure_for(&route.provider_id, chrono::Utc::now().timestamp())
                    else {
                        return Err(error.into());
                    };
                    if failure == SupplierFailure::Authentication
                        && refreshed.insert(context.account.id.clone())
                    {
                        match codex2api_scheduler::recover_auth(
                            &state.storage,
                            &state.upstream,
                            &context,
                            revision,
                        )
                        .await?
                        {
                            None => continue,
                            Some(rejection) => failure = rejection,
                        }
                    }
                    codex2api_scheduler::observe_failure(
                        &state.storage,
                        &context.account.id,
                        revision,
                        &failure,
                    )
                    .await?;
                    state
                        .storage
                        .report_supplier_result(&context.account.id, false, None);
                    excluded.push(context.account.id.clone());
                    break;
                }
            }
        }
    }
}

async fn send(
    state: &GatewayState,
    context: &SupplierContext,
    headers: &HeaderMap,
    prepared: &PreparedRequest,
) -> std::result::Result<reqwest::Response, UpstreamError> {
    match context.account.provider_id.as_str() {
        codex2api_core::CHATGPT => {
            let endpoint = match prepared.upstream_path {
                "responses" => codex2api_upstream::Endpoint::Responses,
                "images/generations" => codex2api_upstream::Endpoint::ImageGeneration,
                "images/edits" => codex2api_upstream::Endpoint::ImageEdit,
                _ => {
                    return Err(UpstreamError::InvalidRequest(
                        "Unsupported ChatGPT API operation".into(),
                    ));
                }
            };
            let mut inbound = headers.clone();
            if !context.reused_affinity {
                inbound.remove("x-codex-turn-state");
            }
            state
                .upstream
                .get(&context.account.id)
                .await?
                .forward_decoded_endpoint(endpoint, None, prepared.body.clone(), inbound)
                .await
        }
        codex2api_core::GROK => {
            state
                .upstream
                .grok()
                .grok_request_prepared(
                    &context.account.id,
                    http::Method::POST,
                    prepared.upstream_path,
                    Some(codex2api_upstream::grok::PreparedBody::new(
                        prepared.body.clone(),
                    )?),
                    headers,
                )
                .await
        }
        _ => Err(UpstreamError::InvalidRequest(
            "Unsupported API platform".into(),
        )),
    }
}

fn public_headers(original: &HeaderMap, stream: bool) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for name in [
        "x-request-id",
        "x-oai-request-id",
        "retry-after",
        "retry-after-ms",
    ] {
        if let Some(value) = original.get(name) {
            headers.insert(name, value.clone());
        }
    }
    headers.insert(
        "content-type",
        if stream {
            "text/event-stream"
        } else {
            "application/json"
        }
        .parse()
        .unwrap(),
    );
    headers.insert("cache-control", "no-store".parse().unwrap());
    headers
}

async fn response_body(
    state: GatewayState,
    mut log: ApiLog,
    permit: ApiPermit,
    context: SupplierContext,
    route: SupplierRoute,
    response: reqwest::Response,
    prepared: PreparedRequest,
) -> Result<Response> {
    let status = response.status();
    let headers = public_headers(response.headers(), prepared.stream);
    log.headers(status, response.headers());
    log.auth_revision = response
        .extensions()
        .get::<codex2api_upstream::SupplierAuthRevision>()
        .map(|revision| revision.0);
    let is_sse = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"));
    if !is_sse {
        let result = read_json(response).await;
        let mut value = match result {
            Ok(value) => value,
            Err(error) => {
                log.fail(&error);
                log.settle().await?;
                return Err(error);
            }
        };
        log.observe(&value, prepared.output);
        observe_terminal_failure(&state, &log, &value).await?;
        if log.lifecycle.outcome().is_none() {
            log.fail(&GatewayError::upstream_incomplete());
        }
        log.settle().await?;
        let success = log.successful();
        if !success {
            return Err(GatewayError::upstream_value(&value));
        }
        if prepared.output == OutputMode::Responses {
            if let Some(id) = value["id"]
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 256)
            {
                let ttl = state.storage.gateway_settings().await?.sticky_ttl_seconds;
                state
                    .storage
                    .save_routing_affinity(
                        &codex2api_scheduler::previous_key(&route.owner_id, &route.provider_id, id),
                        &log.record.account_id,
                        ttl,
                    )
                    .await?;
            }
        }
        if prepared.stream {
            return Err(GatewayError::new(
                StatusCode::BAD_GATEWAY,
                "upstream_protocol_error",
                "The upstream service did not return the requested stream.",
            ));
        }
        if prepared.output == OutputMode::ChatFromResponses {
            value = providers::chatgpt::chat_response(&value)?;
        }
        codex2api_upstream::public_output::metadata(&mut value);
        let mut result = Json(value).into_response();
        *result.headers_mut() = headers;
        drop(context);
        drop(permit);
        return Ok(result);
    }
    if prepared.stream {
        let (sender, receiver) = tokio::sync::mpsc::channel::<std::io::Result<Bytes>>(16);
        tokio::spawn(async move {
            let _permit = permit;
            let _context = context;
            if let Err(error) =
                consume_stream(&state, &route, response, &prepared, &mut log, Some(&sender)).await
            {
                log.fail(&error);
                if let Err(settlement) = log.settle().await {
                    tracing::error!(%settlement,"API stream settlement failed");
                }
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(1),
                    sender.send(Ok(sse(None, &error.body))),
                )
                .await;
            }
        });
        let mut result = Response::new(Body::from_stream(
            tokio_stream::wrappers::ReceiverStream::new(receiver),
        ));
        *result.headers_mut() = headers;
        Ok(result)
    } else {
        let result = consume_stream(&state, &route, response, &prepared, &mut log, None).await;
        match result {
            Ok(value) => {
                let mut result = Json(value).into_response();
                *result.headers_mut() = headers;
                Ok(result)
            }
            Err(error) => {
                log.fail(&error);
                log.settle().await?;
                Err(error)
            }
        }
    }
}

async fn read_json(response: reqwest::Response) -> Result<Value> {
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3600);
    loop {
        let idle_deadline = (tokio::time::Instant::now()
            + codex2api_upstream::DEFAULT_STREAM_IDLE_TIMEOUT)
            .min(deadline);
        let next = tokio::time::timeout_at(idle_deadline, stream.next())
            .await
            .map_err(|_| GatewayError::from(UpstreamError::StreamIdleTimeout))?;
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(UpstreamError::from)?;
        if bytes.len().saturating_add(chunk.len()) > codex2api_upstream::MAX_REQUEST_BYTES {
            return Err(GatewayError::new(
                StatusCode::BAD_GATEWAY,
                "upstream_response_too_large",
                "The upstream response exceeded the size limit.",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| {
        GatewayError::new(
            StatusCode::BAD_GATEWAY,
            "upstream_protocol_error",
            "The upstream response was not valid JSON.",
        )
    })
}

fn sse(event: Option<&str>, value: &Value) -> Bytes {
    Bytes::from(match event {
        Some(event) => format!("event: {event}\ndata: {value}\n\n"),
        None => format!("data: {value}\n\n"),
    })
}
struct Downstream<'a> {
    sender: Option<&'a tokio::sync::mpsc::Sender<std::io::Result<Bytes>>>,
}
impl Downstream<'_> {
    async fn emit(&mut self, bytes: Bytes) {
        if let Some(sender) = self.sender {
            if !matches!(
                tokio::time::timeout(std::time::Duration::from_secs(30), sender.send(Ok(bytes)))
                    .await,
                Ok(Ok(()))
            ) {
                // A stalled receiver must not hold upstream execution or wallet settlement.
                self.sender = None;
            }
        }
    }
}

async fn consume_stream(
    state: &GatewayState,
    route: &SupplierRoute,
    response: reqwest::Response,
    prepared: &PreparedRequest,
    log: &mut ApiLog,
    sender: Option<&tokio::sync::mpsc::Sender<std::io::Result<Bytes>>>,
) -> Result<Value> {
    let events = codex2api_upstream::buffered_sse_lines(response.bytes_stream()).eventsource();
    futures::pin_mut!(events);
    let mut chat = providers::chatgpt::ChatStream::new(prepared.include_usage);
    let mut final_value = None;
    let mut held_chat = vec![];
    let mut seen_response = std::collections::HashSet::new();
    let mut downstream = Downstream { sender };
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3600);
    loop {
        let idle_deadline = (tokio::time::Instant::now()
            + codex2api_upstream::DEFAULT_STREAM_IDLE_TIMEOUT)
            .min(deadline);
        let next = tokio::time::timeout_at(idle_deadline, events.next())
            .await
            .map_err(|_| GatewayError::from(UpstreamError::StreamIdleTimeout))?;
        let Some(event) = next else { break };
        let event = event.map_err(|_| GatewayError::upstream_incomplete())?;
        if event.data.trim() == "[DONE]" {
            log.done(prepared.output);
            if log.lifecycle.outcome().is_none() {
                return Err(GatewayError::upstream_incomplete());
            }
            log.settle().await?;
            if !log.successful() {
                return Err(GatewayError::upstream_value(
                    final_value.as_ref().unwrap_or(&Value::Null),
                ));
            }
            for chunk in held_chat.drain(..) {
                downstream.emit(chunk).await;
            }
            if prepared.output == OutputMode::ChatCompletions {
                downstream
                    .emit(Bytes::from_static(b"data: [DONE]\n\n"))
                    .await;
            }
            break;
        }
        let mut value: Value =
            serde_json::from_str(&event.data).map_err(|_| GatewayError::upstream_incomplete())?;
        if value["type"]
            .as_str()
            .is_some_and(|kind| kind.contains("rate_limit") || kind.contains("billing"))
        {
            continue;
        }
        let terminal = log.observe(&value, prepared.output);
        if let Some(id) = value
            .pointer("/response/id")
            .and_then(Value::as_str)
            .filter(|id| id.len() <= 256)
        {
            if seen_response.insert(id.to_owned()) {
                let ttl = state.storage.gateway_settings().await?.sticky_ttl_seconds;
                state
                    .storage
                    .save_routing_affinity(
                        &codex2api_scheduler::previous_key(&route.owner_id, &route.provider_id, id),
                        &log.record.account_id,
                        ttl,
                    )
                    .await?;
            }
        }
        if terminal {
            final_value = Some(value.get("response").unwrap_or(&value).clone());
            observe_terminal_failure(state, log, &value).await?;
            log.settle().await?;
        }
        codex2api_upstream::public_output::metadata(&mut value);
        match prepared.output {
            OutputMode::ChatFromResponses => {
                for chunk in chat.event(&value)? {
                    downstream.emit(sse(None, &chunk)).await;
                }
                if terminal {
                    downstream
                        .emit(Bytes::from_static(b"data: [DONE]\n\n"))
                        .await;
                    break;
                }
            }
            OutputMode::ChatCompletions => {
                if terminal {
                    return Err(GatewayError::upstream_value(&value));
                }
                let trailer = value["choices"].as_array().is_some_and(Vec::is_empty)
                    && value["usage"].is_object();
                if !prepared.include_usage {
                    if trailer {
                        continue;
                    }
                    if let Some(object) = value.as_object_mut() {
                        object.remove("usage");
                    }
                }
                let chunk = sse(None, &value);
                if log.chat_finished {
                    if held_chat
                        .iter()
                        .map(Bytes::len)
                        .sum::<usize>()
                        .saturating_add(chunk.len())
                        > codex2api_upstream::MAX_REQUEST_BYTES
                    {
                        return Err(GatewayError::upstream_incomplete());
                    }
                    held_chat.push(chunk);
                } else {
                    downstream.emit(chunk).await;
                }
            }
            OutputMode::Responses => {
                let kind = value["type"].as_str().filter(|kind| {
                    kind.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'_')
                });
                downstream.emit(sse(kind, &value)).await;
                if terminal {
                    break;
                }
            }
            OutputMode::Images => {
                return Err(GatewayError::new(
                    StatusCode::BAD_GATEWAY,
                    "upstream_protocol_error",
                    "Image generation returned an unsupported stream.",
                ));
            }
        }
    }
    if log.lifecycle.outcome().is_none() {
        return Err(GatewayError::upstream_incomplete());
    }
    log.settle().await?;
    if let Some(ResponseOutcome::Failed(_) | ResponseOutcome::Cancelled) = log.lifecycle.outcome() {
        if sender.is_none() {
            return Err(GatewayError::upstream_value(
                final_value.as_ref().unwrap_or(&Value::Null),
            ));
        }
    }
    let value = final_value.unwrap_or(Value::Null);
    if sender.is_some() {
        return Ok(Value::Null);
    }
    let mut value = if prepared.output == OutputMode::ChatFromResponses {
        providers::chatgpt::chat_response(&value)?
    } else {
        value
    };
    codex2api_upstream::public_output::metadata(&mut value);
    Ok(value)
}

async fn observe_terminal_failure(state: &GatewayState, log: &ApiLog, value: &Value) -> Result<()> {
    if let Some(failure) = codex2api_upstream::classify_provider_failure(
        &log.record.provider_id,
        None,
        value,
        &HeaderMap::new(),
        chrono::Utc::now().timestamp(),
    ) {
        codex2api_scheduler::observe_failure(
            &state.storage,
            &log.record.account_id,
            log.auth_revision,
            &failure,
        )
        .await?;
    }
    Ok(())
}

use crate::{
    ApiError, ApiState, OutputMode, PreparedRequest, Result, providers,
    runtime::{ApiPermit, Limits},
    usage::ApiLog,
};
use axum::{
    Json,
    body::{Body, Bytes},
    response::{IntoResponse, Response},
};
use codex2api_storage::{ApiPrincipal, UsageRecord};
use codex2api_upstream::scheduler::{SupplierContext, SupplierRoute};
use codex2api_upstream::{ResponseOutcome, SupplierFailure, UpstreamError};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

pub(crate) async fn execute_prepared(
    state: ApiState,
    principal: ApiPrincipal,
    token: String,
    headers: HeaderMap,
    prepared: PreparedRequest,
) -> Result<Response> {
    // The accepted request and its usage continue if the downstream disconnects.
    tokio::spawn(codex2api_upstream::RequestFailureLog::inherit(run(
        state, principal, token, headers, prepared,
    )))
    .await
    .map_err(|error| {
        tracing::error!(%error,"API request worker failed");
        ApiError::internal()
    })?
}

async fn run(
    state: ApiState,
    principal: ApiPrincipal,
    token: String,
    headers: HeaderMap,
    prepared: PreparedRequest,
) -> Result<Response> {
    let user_agent = codex2api_upstream::request_user_agent(&headers);
    let (mut log, permit) =
        admit(&state, &principal, &prepared, "http", user_agent.as_deref()).await?;
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

/// HTTP requests and each WebSocket generation share one admission and ledger.
pub(crate) async fn admit(
    state: &ApiState,
    principal: &ApiPrincipal,
    prepared: &PreparedRequest,
    transport: &str,
    user_agent: Option<&str>,
) -> Result<(ApiLog, ApiPermit)> {
    let metadata = codex2api_upstream::request_metadata_value(&prepared.body);
    let model = metadata
        .model
        .as_deref()
        .filter(|model| codex2api_common::valid_model(model))
        .ok_or_else(|| ApiError::bad_request("model must name a configured model"))?;
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
    let clock = codex2api_upstream::RequestClock::current().unwrap_or_default();
    let started = clock.started;
    let record = UsageRecord {
        id: uuid::Uuid::new_v4().to_string(),
        provider_id: principal.group.provider_id.clone(),
        subject_id: principal.key_id.clone(),
        subject_kind: "api_key".into(),
        endpoint: prepared.endpoint.into(),
        transport: transport.into(),
        user_agent: user_agent.map(str::to_owned),
        stream: Some(prepared.stream),
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
        requested_at_ms: clock.requested_at_ms,
        status: "in_progress".into(),
        ..Default::default()
    };
    state.storage.admit_api_request(principal, &record).await?;
    let mut log = ApiLog::new(state.storage.clone(), record, started);
    let permit = match state
        .runtime
        .acquire(
            &principal.user_id,
            &principal.key_id,
            &principal.group.id,
            &model,
            limits,
        )
        .await
    {
        Ok(permit) => permit,
        Err(error) => {
            let error = ApiError::from(error);
            log.fail(&error);
            log.settle().await?;
            return Err(error);
        }
    };
    Ok((log, permit))
}

pub(crate) async fn validate(
    state: &ApiState,
    principal: &ApiPrincipal,
    token: &str,
) -> codex2api_upstream::scheduler::Result<()> {
    let current = state
        .storage
        .authenticate_api_key(token)
        .await?
        .ok_or(codex2api_upstream::scheduler::SchedulerError::ConsumerInvalid)?;
    if current.user_id != principal.user_id
        || current.key_id != principal.key_id
        || current.group.id != principal.group.id
        || current.group.revision != principal.group.revision
    {
        return Err(codex2api_upstream::scheduler::SchedulerError::ConsumerInvalid);
    }
    Ok(())
}

pub(crate) fn route(principal: &ApiPrincipal) -> SupplierRoute {
    SupplierRoute {
        owner_id: format!(
            "api:{}:{}:{}",
            principal.user_id, principal.key_id, principal.group.id
        ),
        provider_id: principal.group.provider_id.clone(),
        tag_ids: principal.group.tag_ids.clone(),
    }
}

/// Turn state is an opaque supplier capability. Only replay a state that this
/// caller received from the selected supplier; never derive new conversation IDs.
pub(crate) async fn owned_headers(
    state: &ApiState,
    route: &SupplierRoute,
    context: &SupplierContext,
    inbound: &HeaderMap,
) -> codex2api_upstream::Result<HeaderMap> {
    let mut headers = inbound.clone();
    if let Some(turn) = headers.get("x-codex-turn-state") {
        let key = codex2api_storage::RoutingKey::turn(
            &route.owner_id,
            &route.provider_id,
            turn.to_str().unwrap_or(""),
        );
        if state.storage.routing_affinity(&key).await?.as_deref()
            != Some(context.account.id.as_str())
        {
            headers.remove("x-codex-turn-state");
        }
    }
    Ok(headers)
}

pub(crate) async fn remember_affinity(
    state: &ApiState,
    route: &SupplierRoute,
    account: &str,
    response_id: Option<&str>,
    headers: Option<&HeaderMap>,
) -> Result<()> {
    let ttl = state.storage.gateway_settings().await?.sticky_ttl_seconds;
    let response = response_id
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .map(|id| codex2api_storage::RoutingKey::response(&route.owner_id, &route.provider_id, id));
    let turn = headers
        .and_then(|h| h.get("x-codex-turn-state"))
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= 16384)
        .map(|v| codex2api_storage::RoutingKey::turn(&route.owner_id, &route.provider_id, v));
    let bindings: Vec<_> = response
        .iter()
        .chain(turn.iter())
        .map(|key| (key, account))
        .collect();
    state
        .storage
        .save_routing_affinities(&bindings, ttl)
        .await?;
    Ok(())
}

async fn forward(
    state: &ApiState,
    principal: &ApiPrincipal,
    token: &str,
    headers: &HeaderMap,
    prepared: &PreparedRequest,
    log: &mut ApiLog,
) -> Result<(reqwest::Response, SupplierContext, SupplierRoute)> {
    let route = route(principal);
    let schedule = match route.provider_id.as_str() {
        codex2api_common::CHATGPT => providers::chatgpt::scheduling(headers, &prepared.body),
        codex2api_common::GROK => providers::grok::scheduling(headers, &prepared.body),
        _ => return Err(ApiError::bad_request("Unsupported API platform")),
    };
    let mut excluded = vec![];
    let mut refreshed = std::collections::HashSet::new();
    loop {
        validate(state, principal, token).await?;
        let context = codex2api_upstream::scheduler::select(
            &state.storage,
            &route,
            &schedule,
            &excluded,
            || validate(state, principal, token),
        )
        .await?;
        log.record.account_id = context.account.id.clone();
        log.record.account_name = context
            .account
            .display_name
            .clone()
            .unwrap_or_else(|| context.account.email.clone().unwrap_or_default());
        // A refresh retries this supplier once without releasing its reserved slot.
        loop {
            let mut revision = state
                .storage
                .supplier_auth_revision(&context.account.id)
                .await?;
            log.record.http_status = None;
            let result = async {
                let inbound = owned_headers(state, &route, &context, &prepared.headers).await?;
                let response = send(state, &context, &inbound, prepared).await?;
                revision = response
                    .extensions()
                    .get::<codex2api_upstream::SupplierAuthRevision>()
                    .map(|r| r.0)
                    .or(revision);
                let prelude = if route.provider_id == codex2api_common::GROK {
                    providers::grok::prelude
                } else {
                    codex2api_upstream::scheduler::prelude_event
                };
                codex2api_upstream::scheduler::inspect_response(
                    &route.provider_id,
                    prelude,
                    response,
                )
                .await
            }
            .await;
            match result {
                Ok(response) => {
                    log.auth_revision = revision;
                    return Ok((response, context, route));
                }
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
                            if codex2api_upstream::scheduler::generation_observed(&value) {
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
                                    codex2api_upstream::scheduler::observe_failure(
                                        &state.storage,
                                        &context.account.id,
                                        error.auth_revision().or(revision),
                                        &failure,
                                    )
                                    .await?;
                                }
                                // Output or usage proves this attempt ran. Never duplicate it on
                                // another supplier or refresh and execute the request again.
                                log.fail_upstream(&error);
                                return Err(error.into());
                            }
                        }
                    }
                    revision = error.auth_revision().or(revision);
                    let Some(mut failure) = error
                        .supplier_failure_for(&route.provider_id, chrono::Utc::now().timestamp())
                    else {
                        log.fail_upstream(&error);
                        return Err(error.into());
                    };
                    if failure == SupplierFailure::Authentication
                        && refreshed.insert(context.account.id.clone())
                    {
                        match codex2api_upstream::scheduler::recover_auth(
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
                    codex2api_upstream::scheduler::observe_failure(
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
    state: &ApiState,
    context: &SupplierContext,
    headers: &HeaderMap,
    prepared: &PreparedRequest,
) -> std::result::Result<reqwest::Response, UpstreamError> {
    match context.account.provider_id.as_str() {
        codex2api_common::CHATGPT => {
            let endpoint = match prepared.upstream_path {
                "responses" => codex2api_upstream::Endpoint::Responses,
                "responses/compact" => codex2api_upstream::Endpoint::Compact,
                "responses/input_tokens" => codex2api_upstream::Endpoint::InputTokens,
                "alpha/search" => codex2api_upstream::Endpoint::Search,
                "images/generations" => codex2api_upstream::Endpoint::ImageGeneration,
                "images/edits" => codex2api_upstream::Endpoint::ImageEdit,
                _ => {
                    return Err(UpstreamError::InvalidRequest(
                        "Unsupported ChatGPT API operation".into(),
                    ));
                }
            };
            state
                .upstream
                .get(&context.account.id)
                .await?
                .forward_decoded_endpoint(endpoint, None, prepared.body.clone(), headers.clone())
                .await
        }
        codex2api_common::GROK => {
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

pub(crate) fn public_headers(original: &HeaderMap, stream: bool) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for name in [
        "x-request-id",
        "x-oai-request-id",
        "retry-after",
        "retry-after-ms",
        "x-codex-turn-state",
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
    state: ApiState,
    mut log: ApiLog,
    permit: ApiPermit,
    context: SupplierContext,
    route: SupplierRoute,
    response: reqwest::Response,
    prepared: PreparedRequest,
) -> Result<Response> {
    let status = response.status();
    let headers = public_headers(response.headers(), prepared.stream);
    log.set_timing(
        response
            .extensions()
            .get::<codex2api_upstream::ResponseTiming>()
            .cloned(),
    );
    log.headers(status, response.headers());
    remember_affinity(
        &state,
        &route,
        &log.record.account_id,
        None,
        Some(response.headers()),
    )
    .await?;
    log.auth_revision = response
        .extensions()
        .get::<codex2api_upstream::SupplierAuthRevision>()
        .map(|revision| revision.0)
        .or(log.auth_revision);
    let is_sse = codex2api_upstream::ResponseFormat::from_headers(response.headers())
        == codex2api_upstream::ResponseFormat::EventStream;
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
        if route.provider_id == codex2api_common::GROK {
            codex2api_upstream::grok::errors::normalize_response(&mut value);
        }
        log.measure(&value, prepared.output);
        if matches!(
            ResponseOutcome::from_value(&value, None),
            Some(ResponseOutcome::Failed(_) | ResponseOutcome::Cancelled)
        ) {
            log.observe(&value, prepared.output);
            observe_terminal_failure(&state, &log, &value).await?;
            log.settle().await?;
            let mut error = ApiError::upstream_value(&value);
            error.headers.extend(headers);
            return Err(error);
        }
        // Validate the requested transport before accepting a success-shaped body.
        if prepared.stream {
            let error = ApiError::new(
                StatusCode::BAD_GATEWAY,
                "upstream_protocol_error",
                "The upstream service did not return the requested stream.",
            );
            log.fail(&error);
            log.settle().await?;
            return Err(error);
        }
        log.observe(&value, prepared.output);
        observe_terminal_failure(&state, &log, &value).await?;
        if log.lifecycle.outcome().is_none() {
            log.fail(&ApiError::upstream_incomplete());
        }
        let success = log.successful();
        if !success {
            log.settle().await?;
            let mut error = ApiError::upstream_value(&value);
            error.headers.extend(headers);
            return Err(error);
        }
        if prepared.output == OutputMode::Responses {
            remember_affinity(
                &state,
                &route,
                &log.record.account_id,
                value["id"].as_str(),
                None,
            )
            .await?;
        }
        let mut value = if prepared.output == OutputMode::ChatFromResponses {
            match providers::chatgpt::chat_response(&value) {
                Ok(value) => value,
                Err(error) => {
                    log.fail(&error);
                    log.settle().await?;
                    return Err(error);
                }
            }
        } else {
            value
        };
        codex2api_upstream::public_output::metadata(&mut value);
        if let Some(metadata) = &prepared.metadata {
            value["metadata"] = metadata.clone();
        }
        log.settle().await?;
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
                    sender.send(Ok(sse(Some("error"), &error_event(&error)))),
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
    let media_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(str::trim)
        .unwrap_or("missing");
    let media_type = match media_type {
        "application/json" | "text/html" | "text/plain" | "missing" => media_type,
        _ => "other",
    }
    .to_owned();
    let encoding = response
        .headers()
        .get("content-encoding")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("identity");
    let encoding = match encoding {
        "identity" | "gzip" | "br" | "deflate" | "zstd" => encoding,
        _ => "other",
    }
    .to_owned();
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3600);
    loop {
        let idle_deadline = (tokio::time::Instant::now()
            + codex2api_upstream::DEFAULT_STREAM_IDLE_TIMEOUT)
            .min(deadline);
        let next = tokio::time::timeout_at(idle_deadline, stream.next())
            .await
            .map_err(|_| ApiError::from(UpstreamError::StreamIdleTimeout))?;
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(UpstreamError::from)?;
        if bytes.len().saturating_add(chunk.len()) > codex2api_upstream::MAX_REQUEST_BYTES {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "upstream_response_too_large",
                "The upstream response exceeded the size limit.",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
    serde_json::from_slice(bytes).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "upstream_protocol_error",
            format!("The upstream response was not valid JSON (content-type: {media_type}; encoding: {encoding}; bytes: {}; line: {}; column: {}).", bytes.len(), error.line(), error.column()),
        )
    })
}

fn sse(event: Option<&str>, value: &Value) -> Bytes {
    Bytes::from(match event {
        Some(event) => format!("event: {event}\ndata: {value}\n\n"),
        None => format!("data: {value}\n\n"),
    })
}

pub(crate) fn error_event(error: &ApiError) -> Value {
    let mut value = error.body.clone();
    value["type"] = "error".into();
    value["status"] = error.status.as_u16().into();
    for name in ["retry-after", "retry-after-ms", "x-should-retry"] {
        if let Some(retry) = error.headers.get(name).and_then(|v| v.to_str().ok()) {
            value["headers"][name] = retry.into();
        }
    }
    value
}

/// Hold complete output items only when assembling a synchronous response, or to
/// complete a terminal envelope that omits its already-streamed output.
#[derive(Default)]
pub(crate) struct ResponsesOutput {
    response: serde_json::Map<String, Value>,
    items: std::collections::BTreeMap<u64, (Value, usize)>,
    bytes: usize,
}
impl ResponsesOutput {
    pub(crate) fn observe(&mut self, value: &mut Value) -> Result<()> {
        if value["type"] == "response.output_item.done"
            && let (Some(index), Some(item)) = (value["output_index"].as_u64(), value.get("item"))
        {
            let bytes = serde_json::to_vec(item)
                .map_err(|_| ApiError::internal())?
                .len();
            self.bytes = self
                .bytes
                .saturating_sub(self.items.get(&index).map_or(0, |(_, n)| *n))
                .saturating_add(bytes);
            if self.bytes > codex2api_upstream::MAX_REQUEST_BYTES {
                return Err(ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "upstream_response_too_large",
                    "The upstream response exceeded the size limit.",
                ));
            }
            self.items.insert(index, (item.clone(), bytes));
        }
        let terminal = ResponseOutcome::from_value(value, None).is_some();
        if let Some(response) = value.get_mut("response").and_then(Value::as_object_mut) {
            if terminal {
                for (key, item) in std::mem::take(&mut self.response) {
                    response.entry(key).or_insert(item);
                }
                if !self.items.is_empty() {
                    let mut items: std::collections::BTreeMap<u64, Value> = response
                        .remove("output")
                        .and_then(|value| value.as_array().cloned())
                        .unwrap_or_default()
                        .into_iter()
                        .enumerate()
                        .map(|(index, value)| (index as u64, value))
                        .collect();
                    items.extend(
                        std::mem::take(&mut self.items)
                            .into_iter()
                            .map(|(index, (value, _))| (index, value)),
                    );
                    response.insert("output".into(), Value::Array(items.into_values().collect()));
                }
            } else {
                self.response.extend(
                    response
                        .iter()
                        .filter(|(key, _)| !matches!(key.as_str(), "output" | "status"))
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
            }
        }
        Ok(())
    }
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
    state: &ApiState,
    route: &SupplierRoute,
    response: reqwest::Response,
    prepared: &PreparedRequest,
    log: &mut ApiLog,
    sender: Option<&tokio::sync::mpsc::Sender<std::io::Result<Bytes>>>,
) -> Result<Value> {
    let events = codex2api_upstream::buffered_sse_lines(response.bytes_stream()).eventsource();
    futures::pin_mut!(events);
    let mut chat = providers::chatgpt::ChatStream::new(prepared.include_usage);
    let mut output = ResponsesOutput::default();
    let mut final_value = None;
    let mut held_chat = Vec::new();
    let mut held_bytes = 0usize;
    let mut seen_response = std::collections::HashSet::new();
    let mut downstream = Downstream { sender };
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3600);
    loop {
        let idle_deadline = (tokio::time::Instant::now()
            + codex2api_upstream::DEFAULT_STREAM_IDLE_TIMEOUT)
            .min(deadline);
        let next = tokio::time::timeout_at(idle_deadline, events.next())
            .await
            .map_err(|_| ApiError::from(UpstreamError::StreamIdleTimeout))?;
        let Some(event) = next else { break };
        let event = event
            .map_err(|error| ApiError::upstream_incomplete().with_diagnostic(&error.to_string()))?;
        if event.data.trim() == "[DONE]" {
            if sender.is_none() && prepared.output == OutputMode::ChatCompletions {
                return Err(ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "upstream_protocol_error",
                    "The upstream service returned a stream for a synchronous Chat request.",
                ));
            }
            log.done(prepared.output);
            if log.lifecycle.outcome().is_none() {
                return Err(ApiError::upstream_incomplete());
            }
            log.settle().await?;
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
        if event.data.len() > codex2api_upstream::MAX_REQUEST_BYTES {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "upstream_response_too_large",
                "The upstream event exceeded the size limit.",
            ));
        }
        let mut value: Value = serde_json::from_str(&event.data).map_err(|_| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "upstream_protocol_error",
                "The upstream SSE event was not valid JSON.",
            )
        })?;
        if !value.is_object() {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "upstream_protocol_error",
                "The upstream SSE event was not an object.",
            ));
        }
        if value.get("type").is_none() && !event.event.is_empty() && event.event != "message" {
            value["type"] = event.event.into();
        }
        if route.provider_id == codex2api_common::GROK {
            codex2api_upstream::grok::errors::normalize_response(&mut value);
        }
        if matches!(
            value["type"].as_str(),
            Some("codex.rate_limits" | "rate_limits.updated")
        ) {
            continue;
        }
        if matches!(
            prepared.output,
            OutputMode::Responses | OutputMode::ChatFromResponses
        ) {
            output.observe(&mut value)?;
        }
        if let Some(id) = value
            .pointer("/response/id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
            && seen_response.insert(id.to_owned())
        {
            remember_affinity(state, route, &log.record.account_id, Some(id), None).await?;
        }
        let terminal = log.observe(&value, prepared.output);
        if terminal {
            if sender.is_none() {
                final_value = Some(value.get("response").unwrap_or(&value).clone());
            }
            observe_terminal_failure(state, log, &value).await?;
        }
        // Prepare every terminal payload before committing success. Conversion
        // failures remain failed requests; persistence always precedes delivery.
        let failure = matches!(
            log.lifecycle.outcome(),
            Some(ResponseOutcome::Failed(_) | ResponseOutcome::Cancelled)
        );
        codex2api_upstream::public_output::metadata(&mut value);
        if let Some(metadata) = &prepared.metadata {
            if let Some(response) = value.get_mut("response").filter(|v| v.is_object()) {
                response["metadata"] = metadata.clone();
            }
        }
        let chunks = match prepared.output {
            OutputMode::ChatFromResponses if failure => {
                vec![sse(None, &error_event(&ApiError::upstream_value(&value)))]
            }
            OutputMode::ChatFromResponses if downstream.sender.is_some() => chat
                .event(&value)?
                .iter()
                .map(|value| sse(None, value))
                .collect(),
            OutputMode::ChatFromResponses => Vec::new(),
            OutputMode::ChatCompletions => {
                if failure {
                    vec![sse(None, &error_event(&ApiError::upstream_value(&value)))]
                } else {
                    let trailer = value["choices"].as_array().is_some_and(Vec::is_empty)
                        && value["usage"].is_object();
                    if !prepared.include_usage {
                        if trailer {
                            continue;
                        }
                        value.as_object_mut().unwrap().remove("usage");
                    }
                    let chunk = sse(None, &value);
                    if log.chat_finished {
                        held_bytes = held_bytes.saturating_add(chunk.len());
                        if held_bytes > codex2api_upstream::MAX_REQUEST_BYTES {
                            return Err(ApiError::upstream_incomplete());
                        }
                        held_chat.push(chunk);
                        Vec::new()
                    } else {
                        vec![chunk]
                    }
                }
            }
            OutputMode::Responses => {
                let kind = value["type"].as_str().filter(|kind| {
                    kind.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'_')
                });
                vec![sse(kind, &value)]
            }
            OutputMode::Images
            | OutputMode::Compact
            | OutputMode::InputTokens
            | OutputMode::Search => {
                return Err(if failure {
                    ApiError::upstream_value(&value)
                } else {
                    ApiError::new(
                        StatusCode::BAD_GATEWAY,
                        "upstream_protocol_error",
                        "The upstream operation returned a stream instead of a JSON response.",
                    )
                });
            }
        };
        if terminal {
            // A synchronous Chat response is also validated before settlement.
            if sender.is_none() && prepared.output == OutputMode::ChatFromResponses && !failure {
                final_value = Some(providers::chatgpt::chat_response(
                    final_value.as_ref().unwrap(),
                )?);
            }
            log.settle().await?;
        }
        for chunk in chunks {
            downstream.emit(chunk).await;
        }
        if terminal {
            if prepared.output == OutputMode::ChatFromResponses && !failure {
                downstream
                    .emit(Bytes::from_static(b"data: [DONE]\n\n"))
                    .await;
            }
            break;
        }
    }
    if log.lifecycle.outcome().is_none() {
        return Err(ApiError::upstream_incomplete());
    }
    log.settle().await?;
    if sender.is_some() {
        return Ok(Value::Null);
    }
    let value = final_value.unwrap_or(Value::Null);
    if !log.successful() {
        return Err(ApiError::upstream_value(&value));
    }
    let mut value = value;
    codex2api_upstream::public_output::metadata(&mut value);
    if let Some(metadata) = &prepared.metadata {
        value["metadata"] = metadata.clone();
    }
    Ok(value)
}
async fn observe_terminal_failure(state: &ApiState, log: &ApiLog, value: &Value) -> Result<()> {
    if let Some(failure) = codex2api_upstream::classify_provider_failure(
        &log.record.provider_id,
        None,
        value,
        &HeaderMap::new(),
        chrono::Utc::now().timestamp(),
    ) {
        codex2api_upstream::scheduler::observe_failure(
            &state.storage,
            &log.record.account_id,
            log.auth_revision,
            &failure,
        )
        .await?;
    }
    Ok(())
}

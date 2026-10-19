//! Downstream delivery and settlement. Supplier events are measured in their own
//! protocol, converted once into the caller's protocol, and settled before the
//! caller receives a terminal event. Protocol transforms are upstream-owned.
use crate::{
    ApiError, ApiState, PreparedRequest, Protocol, Result, dispatch::remember_affinity,
    runtime::ApiPermit, usage::ApiLog,
};
use axum::{
    Json,
    body::{Body, Bytes},
    response::{IntoResponse, Response},
};
use codex2api_upstream::protocol::{self, Collector, Converter};
use codex2api_upstream::{
    ResponseOutcome, UpstreamError,
    scheduler::{SupplierContext, SupplierRoute},
};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

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

pub(crate) async fn response_body(
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
        let result = unary(&state, &route, &mut log, response, &prepared).await;
        return match result {
            Ok(value) => {
                let mut result = Json(value).into_response();
                *result.headers_mut() = headers;
                drop(context);
                drop(permit);
                Ok(result)
            }
            Err(mut error) => {
                log.fail(&error);
                log.settle().await?;
                error.headers.extend(headers);
                Err(error)
            }
        };
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
                    sender.send(Ok(frame(
                        prepared.inbound,
                        &failure_event(prepared.inbound, &error),
                    ))),
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
        match consume_stream(&state, &route, response, &prepared, &mut log, None).await {
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

/// JSON replies: unary operations and failures. Generations are always requested as streams.
async fn unary(
    state: &ApiState,
    route: &SupplierRoute,
    log: &mut ApiLog,
    response: reqwest::Response,
    prepared: &PreparedRequest,
) -> Result<Value> {
    let mut value = read_json(response).await?;
    codex2api_upstream::providers::normalize_event(&route.provider_id, &mut value);
    log.measure(&value, prepared.upstream);
    let failed = matches!(
        ResponseOutcome::from_value(&value, None),
        Some(ResponseOutcome::Failed(_) | ResponseOutcome::Cancelled)
    );
    if failed || prepared.upstream.generation() {
        log.observe(&value, prepared.upstream);
        observe_terminal_failure(state, log, &value).await?;
        if failed {
            log.settle().await?;
            return Err(ApiError::upstream_value(&value));
        }
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "upstream_protocol_error",
            "The upstream service did not return the requested stream.",
        ));
    }
    log.observe(&value, prepared.upstream);
    observe_terminal_failure(state, log, &value).await?;
    if log.lifecycle.outcome().is_none() {
        return Err(ApiError::upstream_incomplete());
    }
    if !log.successful() {
        log.settle().await?;
        return Err(ApiError::upstream_value(&value));
    }
    codex2api_upstream::public_output::metadata(&mut value);
    if let Some(metadata) = &prepared.metadata {
        value["metadata"] = metadata.clone();
    }
    log.settle().await?;
    Ok(value)
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
            return Err(too_large());
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

fn too_large() -> ApiError {
    ApiError::new(
        StatusCode::BAD_GATEWAY,
        "upstream_response_too_large",
        "The upstream response exceeded the size limit.",
    )
}

/// One SSE frame, serialized once. Chat chunks have no event name.
fn frame(protocol: Protocol, value: &Value) -> Bytes {
    let mut buffer = Vec::with_capacity(128);
    if protocol != Protocol::ChatCompletions
        && let Some(kind) = value["type"].as_str().filter(|s| {
            s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
        })
    {
        buffer.extend_from_slice(b"event: ");
        buffer.extend_from_slice(kind.as_bytes());
        buffer.push(b'\n');
    }
    buffer.extend_from_slice(b"data: ");
    serde_json::to_writer(&mut buffer, value).expect("JSON value");
    buffer.extend_from_slice(b"\n\n");
    Bytes::from(buffer)
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

/// An in-stream error in the caller's protocol.
fn failure_event(inbound: Protocol, error: &ApiError) -> Value {
    if inbound == Protocol::Messages {
        protocol::messages::error(error.status, &error.body)
    } else {
        error_event(error)
    }
}

struct Downstream<'a> {
    sender: Option<&'a tokio::sync::mpsc::Sender<std::io::Result<Bytes>>>,
}
impl Downstream<'_> {
    async fn emit(&mut self, bytes: Bytes) {
        if let Some(sender) = self.sender
            && !matches!(
                tokio::time::timeout(std::time::Duration::from_secs(30), sender.send(Ok(bytes)))
                    .await,
                Ok(Ok(()))
            )
        {
            // A stalled receiver must not hold upstream execution or wallet settlement.
            self.sender = None;
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
    // Every adapter delivers Responses events; Responses callers receive them as they are.
    let native = prepared.inbound == Protocol::Responses;
    let mut converter = Converter::new(prepared);
    let mut collector = sender.is_none().then(|| Collector::new(prepared.inbound));
    // A native Responses stream may omit already-streamed items from its terminal envelope.
    let mut items = (sender.is_some() && native).then(protocol::responses::Output::default);
    let mut reply = None;
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
        // Responses ends with a response event; a bare sentinel ends a truncated stream.
        if event.data.trim() == "[DONE]" {
            break;
        }
        if event.data.len() > codex2api_upstream::MAX_REQUEST_BYTES {
            return Err(too_large());
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
        codex2api_upstream::providers::normalize_event(&route.provider_id, &mut value);
        if matches!(
            value["type"].as_str(),
            Some("codex.rate_limits" | "rate_limits.updated")
        ) {
            continue;
        }
        if let Some(items) = &mut items {
            items.observe(&mut value)?;
        }
        if let Some(id) = value
            .pointer("/response/id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256)
            && seen_response.insert(id.to_owned())
        {
            remember_affinity(state, route, &log.record.account_id, Some(id), None).await?;
        }
        let terminal = log.observe(&value, prepared.upstream);
        if terminal {
            observe_terminal_failure(state, log, &value).await?;
        }
        let failure = matches!(
            log.lifecycle.outcome(),
            Some(ResponseOutcome::Failed(_) | ResponseOutcome::Cancelled)
        )
        .then(|| ApiError::upstream_value(&value));
        codex2api_upstream::public_output::metadata(&mut value);
        let mut chunks = match &failure {
            // Native Responses callers receive the supplier's own failure event.
            Some(_) if native => vec![value],
            Some(error) => vec![failure_event(prepared.inbound, error)],
            None => converter.event(value)?,
        };
        let terminal = terminal || failure.is_some();
        if prepared.inbound == Protocol::Responses
            && let Some(metadata) = &prepared.metadata
        {
            for chunk in &mut chunks {
                if let Some(response) = chunk.get_mut("response").filter(|v| v.is_object()) {
                    response["metadata"] = metadata.clone();
                }
            }
        }
        if let Some(collector) = &mut collector
            && failure.is_none()
        {
            for chunk in chunks.drain(..) {
                collector.push(chunk)?;
            }
        }
        if terminal {
            // Prepare the caller's terminal payload before committing success.
            if failure.is_none()
                && let Some(collector) = collector.take()
            {
                reply = Some(collector.finish()?);
            }
            log.settle().await?;
            if let Some(error) = failure
                && sender.is_none()
            {
                return Err(error);
            }
        }
        for chunk in &chunks {
            downstream.emit(frame(prepared.inbound, chunk)).await;
        }
        if terminal {
            if prepared.inbound == Protocol::ChatCompletions && log.successful() {
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
    let mut value = reply.ok_or_else(ApiError::upstream_incomplete)?;
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

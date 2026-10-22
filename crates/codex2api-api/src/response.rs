//! Downstream delivery and settlement. Wire events are observed for the ledger in
//! the wire's vocabulary, converted once into the caller's protocol, and settled
//! before the caller receives a terminal event.
use crate::{
    ApiError, ApiState, InboundRequest, Result, dispatch::remember_affinity, runtime::ApiPermit,
    usage::ApiLog,
};
use axum::{
    Json,
    body::{Body, Bytes},
    response::{IntoResponse, Response},
};
use codex2api_upstream::{
    UpstreamError, convert, executor::Executed, inbound::Inbound, scheduler::SupplierRoute,
    supply::Wire,
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
        "x-should-retry",
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
    executed: Executed,
    route: SupplierRoute,
    input: InboundRequest,
) -> Result<Response> {
    let Executed {
        response,
        context,
        wire,
        auth_revision,
        ..
    } = executed;
    let status = response.status();
    let headers = public_headers(response.headers(), input.stream);
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
    log.auth_revision = auth_revision.or(log.auth_revision);
    let is_sse = codex2api_upstream::ResponseFormat::from_headers(response.headers())
        == codex2api_upstream::ResponseFormat::EventStream;
    if !is_sse {
        let result = unary(&mut log, response, wire, &input).await;
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
                for name in ["x-request-id", "x-oai-request-id"] {
                    if let Some(value) = headers.get(name) {
                        error.headers.insert(name, value.clone());
                    }
                }
                Err(error)
            }
        };
    }
    if input.stream {
        let (sender, receiver) = tokio::sync::mpsc::channel::<std::io::Result<Bytes>>(16);
        tokio::spawn(async move {
            let _permit = permit;
            let _context = context;
            if let Err(error) = consume_stream(
                &state,
                &route,
                response,
                wire,
                &input,
                &mut log,
                Some(&sender),
            )
            .await
            {
                log.fail(&error);
                if let Err(settlement) = log.settle().await {
                    tracing::error!(%settlement,"API stream settlement failed");
                }
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(1),
                    sender.send(Ok(frame(
                        input.format,
                        &failure_event(input.format, &error),
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
        match consume_stream(&state, &route, response, wire, &input, &mut log, None).await {
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
    log: &mut ApiLog,
    response: reqwest::Response,
    wire: Wire,
    input: &InboundRequest,
) -> Result<Value> {
    let mut value = read_json(response).await?;
    let observation = convert::observe(wire, &value);
    let failed = matches!(
        observation.outcome,
        Some(convert::Outcome::Failed { .. } | convert::Outcome::Cancelled)
    );
    if failed || input.operation.generation() {
        log.observe(&observation);
        if failed {
            log.settle().await?;
            return Err(ApiError::wire_failure(input.format, None, &value));
        }
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "upstream_protocol_error",
            "The upstream service did not return the requested stream.",
        ));
    }
    log.measure(&observation);
    value = convert::operation_response(wire, input, value)?;
    log.observe_operation(input.operation, &value);
    if log.lifecycle.outcome().is_none() {
        return Err(ApiError::upstream_incomplete());
    }
    if !log.successful() {
        log.settle().await?;
        return Err(ApiError::wire_failure(input.format, None, &value));
    }
    convert::sanitize::metadata(&mut value);
    if input.format != Inbound::Messages
        && let Some(metadata) = &input.metadata
    {
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
fn frame(inbound: Inbound, value: &Value) -> Bytes {
    let mut buffer = Vec::with_capacity(128);
    if inbound != Inbound::Chat
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
fn failure_event(inbound: Inbound, error: &ApiError) -> Value {
    if inbound == Inbound::Messages {
        codex2api_upstream::inbound::messages::error(error.status, &error.body)
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
    wire: Wire,
    input: &InboundRequest,
    log: &mut ApiLog,
    sender: Option<&tokio::sync::mpsc::Sender<std::io::Result<Bytes>>>,
) -> Result<Value> {
    let events = codex2api_upstream::buffered_sse_lines(response.bytes_stream()).eventsource();
    futures::pin_mut!(events);
    let inbound = input.format;
    let model = input.model().unwrap_or_default().to_owned();
    let mut converter =
        convert::stream::Stream::new(inbound, wire, input.include_usage || !input.stream, &model);
    let mut collector = sender.is_none().then(|| convert::Collector::new(inbound));
    // A native Responses stream may omit already-streamed items from its terminal envelope.
    let mut items = (sender.is_some() && inbound == Inbound::Responses && wire.responses_family())
        .then(convert::responses::output::Output::default);
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
        let converted = match next {
            None => converter.finish()?,
            Some(event) => {
                let event = event.map_err(|error| {
                    ApiError::upstream_incomplete().with_diagnostic(&error.to_string())
                })?;
                if event.data.trim() == "[DONE]" {
                    converter.finish()?
                } else {
                    if event.data.trim().is_empty() {
                        continue;
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
                    if value.get("type").is_none()
                        && !event.event.is_empty()
                        && event.event != "message"
                    {
                        value["type"] = event.event.clone().into();
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
                        remember_affinity(state, route, &log.record.account_id, Some(id), None)
                            .await?;
                    }
                    converter.event(value)?
                }
            }
        };
        let observation = converted.observation;
        let terminal = log.observe(&observation);
        let failure = match &observation.outcome {
            Some(convert::Outcome::Failed { status, body }) => {
                Some(ApiError::wire_failure(inbound, *status, body))
            }
            Some(convert::Outcome::Cancelled) => Some(ApiError::upstream_incomplete()),
            _ => None,
        };
        let mut chunks = match &failure {
            Some(error) => vec![failure_event(inbound, error)],
            None => converted.events,
        };
        for chunk in &mut chunks {
            convert::sanitize::metadata(chunk);
            if inbound == Inbound::Responses
                && let Some(metadata) = &input.metadata
                && let Some(response) = chunk.get_mut("response").filter(|v| v.is_object())
            {
                response["metadata"] = metadata.clone();
            }
        }
        if let Some(collector) = &mut collector
            && failure.is_none()
        {
            for chunk in chunks.drain(..) {
                collector.push(chunk)?;
            }
        }
        let (chunks, completion) = if terminal {
            convert::stream::split_completion(chunks)
        } else {
            (chunks, Vec::new())
        };
        for chunk in &chunks {
            downstream.emit(frame(inbound, chunk)).await;
        }
        if terminal {
            // A final upstream event can release text or queued tool blocks.
            // Only completion waits for the ledger, never that ready content.
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
            for chunk in &completion {
                downstream.emit(frame(inbound, chunk)).await;
            }
            if inbound == Inbound::Chat && log.successful() {
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
    if inbound != Inbound::Messages
        && let Some(metadata) = &input.metadata
    {
        value["metadata"] = metadata.clone();
    }
    Ok(value)
}

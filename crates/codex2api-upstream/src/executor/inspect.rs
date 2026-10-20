//! Prelude inspection: read a reply until its first output. An HTTP error, a
//! transport fault or a failure event before any output is one attempt's failure
//! for the channel to judge; once output or usage is visible the reply belongs to
//! the caller and is replayed unchanged.
use super::AttemptFailure;
use crate::{
    ResponseFormat, UpstreamError,
    supply::{TransportFault, UpstreamFailure, Wire},
};
use bytes::Bytes;
use futures::StreamExt;
use serde_json::Value;

/// Events a Responses-family supplier may emit before any output.
pub fn prelude_event(value: &Value) -> bool {
    matches!(
        value.get("type").and_then(Value::as_str),
        Some("response.created" | "response.in_progress" | "response.queued" | "codex.rate_limits")
    )
}

fn nonempty_array(value: &Value) -> bool {
    value.as_array().is_some_and(|items| !items.is_empty())
}

/// Reported usage or generated output makes replay unsafe, including within an HTTP
/// error body. The same rule reads every wire family's shapes.
pub fn generation_observed(value: &Value) -> bool {
    let response = value.get("response").unwrap_or(value);
    response
        .get("usage")
        .and_then(Value::as_object)
        .is_some_and(|usage| !usage.is_empty())
        || response
            .get("usageMetadata")
            .and_then(Value::as_object)
            .is_some_and(|usage| !usage.is_empty())
        || response
            .get("message")
            .filter(|v| v.is_object())
            .is_some_and(generation_observed)
        || nonempty_array(&response["output"])
        || nonempty_array(&response["data"])
        || nonempty_array(&response["content"])
        || response["candidates"].as_array().is_some_and(|candidates| {
            candidates
                .iter()
                .any(|candidate| nonempty_array(&candidate["content"]["parts"]))
        })
        || matches!(
            value["type"].as_str(),
            Some("content_block_start" | "content_block_delta" | "message_delta")
        )
        || response["choices"].as_array().is_some_and(|choices| {
            choices.iter().any(|choice| {
                let message = choice
                    .get("delta")
                    .or_else(|| choice.get("message"))
                    .unwrap_or(choice);
                message
                    .get("content")
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.is_empty())
                    || message
                        .get("reasoning_content")
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.is_empty())
                    || message.get("function_call").is_some_and(|v| !v.is_null())
                    || nonempty_array(&message["tool_calls"])
            })
        })
}

/// A terminal failure event of a wire's stream, before output.
fn failure_event(wire: Wire, value: &Value) -> bool {
    match wire {
        Wire::Codex | Wire::Xai | Wire::OpenAiResponses | Wire::ChatCompletions => {
            value.get("error").is_some_and(|v| !v.is_null())
                || value
                    .pointer("/response/error")
                    .is_some_and(|v| !v.is_null())
                || matches!(value["type"].as_str(), Some("error" | "response.failed"))
                || value.pointer("/response/status") == Some(&Value::from("failed"))
        }
        Wire::Messages => value["type"] == "error",
        Wire::Gemini => {
            value.get("error").is_some_and(|v| !v.is_null())
                || value
                    .pointer("/response/error")
                    .is_some_and(|v| !v.is_null())
        }
    }
}

/// Whether a wire's event carries nothing the caller must see yet.
fn is_prelude(wire: Wire, value: &Value) -> bool {
    match wire {
        Wire::Codex | Wire::Xai | Wire::OpenAiResponses => prelude_event(value),
        Wire::ChatCompletions => value["choices"].as_array().is_some_and(|choices| {
            choices.iter().all(|choice| {
                let delta = &choice["delta"];
                delta
                    .get("content")
                    .is_none_or(|c| c.as_str().is_some_and(str::is_empty) || c.is_null())
                    && delta.get("tool_calls").is_none_or(Value::is_null)
                    && delta.get("reasoning_content").is_none_or(Value::is_null)
                    && choice["finish_reason"].is_null()
            })
        }),
        Wire::Messages => matches!(value["type"].as_str(), Some("message_start" | "ping")),
        Wire::Gemini => {
            let body = value.get("response").unwrap_or(value);
            body["candidates"].as_array().is_none_or(|candidates| {
                candidates
                    .iter()
                    .all(|candidate| !nonempty_array(&candidate["content"]["parts"]))
            }) && body.get("promptFeedback").is_none()
        }
    }
}

fn decode_error_body(bytes: &[u8]) -> Value {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    serde_json::from_slice(bytes).unwrap_or_else(|_| {
        let text = String::from_utf8_lossy(bytes);
        if text.trim_start().starts_with('<') {
            serde_json::json!({"error":{"message":"Upstream returned an HTML error page"}})
        } else {
            serde_json::json!({"error":{"message":text.trim()}})
        }
    })
}

fn transport_failure(error: UpstreamError, model: Option<&str>) -> AttemptFailure {
    let fault = match &error {
        UpstreamError::StreamIdleTimeout | UpstreamError::Stream(_) => TransportFault::Stream,
        UpstreamError::Http(e) if e.is_timeout() => TransportFault::Timeout,
        UpstreamError::Http(e) if e.is_connect() => TransportFault::Connect,
        UpstreamError::Http(_) => TransportFault::Stream,
        UpstreamError::Refresh(_)
        | UpstreamError::Auth(_)
        | UpstreamError::GrokAuth(_)
        | UpstreamError::AntigravityAuth(_)
        | UpstreamError::ApiAuth(_)
        | UpstreamError::Unauthorized
        | UpstreamError::MissingAccessToken(_) => {
            let failure = error.failure();
            return AttemptFailure {
                failure: UpstreamFailure {
                    status: Some(
                        if error.is_unauthorized()
                            || matches!(error, UpstreamError::MissingAccessToken(_))
                        {
                            401
                        } else {
                            failure.status.unwrap_or(503)
                        },
                    ),
                    headers: error.retry_headers().cloned().unwrap_or_default(),
                    body: serde_json::json!({"error":{"code":failure.code,"message":failure.message}}),
                    transport: None,
                    model: model.map(str::to_owned),
                    output_committed: false,
                    now: chrono::Utc::now().timestamp(),
                },
                error,
            };
        }
        _ => TransportFault::Connect,
    };
    AttemptFailure {
        failure: UpstreamFailure {
            status: None,
            headers: http::HeaderMap::new(),
            body: Value::Null,
            transport: Some(fault),
            model: model.map(str::to_owned),
            output_committed: false,
            now: chrono::Utc::now().timestamp(),
        },
        error,
    }
}

/// The failure a channel error describes, without consuming the error.
pub(crate) fn failure_of(error: &UpstreamError, model: Option<&str>) -> UpstreamFailure {
    if let UpstreamError::Status {
        status,
        body,
        headers,
        ..
    } = error
    {
        let body = decode_error_body(body.as_bytes());
        return UpstreamFailure {
            status: Some(*status),
            headers: headers.clone(),
            output_committed: generation_observed(&body),
            body,
            transport: None,
            model: model.map(str::to_owned),
            now: chrono::Utc::now().timestamp(),
        };
    }
    let fault = match error {
        UpstreamError::StreamIdleTimeout | UpstreamError::Stream(_) => Some(TransportFault::Stream),
        UpstreamError::Http(e) if e.is_timeout() => Some(TransportFault::Timeout),
        UpstreamError::Http(e) if e.is_connect() => Some(TransportFault::Connect),
        UpstreamError::Http(_) => Some(TransportFault::Stream),
        UpstreamError::Refresh(_)
        | UpstreamError::Auth(_)
        | UpstreamError::GrokAuth(_)
        | UpstreamError::AntigravityAuth(_)
        | UpstreamError::ApiAuth(_)
        | UpstreamError::Unauthorized
        | UpstreamError::MissingAccessToken(_) => None,
        _ => Some(TransportFault::Connect),
    };
    match fault {
        Some(fault) => UpstreamFailure {
            status: None,
            headers: http::HeaderMap::new(),
            body: Value::Null,
            transport: Some(fault),
            model: model.map(str::to_owned),
            output_committed: false,
            now: chrono::Utc::now().timestamp(),
        },
        None => {
            let failure = error.failure();
            let unauthorized = error.is_unauthorized()
                || matches!(error, UpstreamError::MissingAccessToken(_))
                || failure.authentication_invalid();
            UpstreamFailure {
                status: Some(if unauthorized {
                    401
                } else {
                    failure.status.unwrap_or(503)
                }),
                headers: error.retry_headers().cloned().unwrap_or_default(),
                body: serde_json::json!({"error":{"code":failure.code,"message":failure.message}}),
                transport: None,
                model: model.map(str::to_owned),
                output_committed: false,
                now: chrono::Utc::now().timestamp(),
            }
        }
    }
}

/// A channel error that is not a reply: credentials, transport, internal faults.
pub(crate) fn send_failure(error: UpstreamError, model: Option<&str>) -> AttemptFailure {
    if let UpstreamError::Status {
        status,
        body,
        headers,
        ..
    } = &error
    {
        let body = decode_error_body(body.as_bytes());
        return AttemptFailure {
            failure: UpstreamFailure {
                status: Some(*status),
                headers: headers.clone(),
                output_committed: generation_observed(&body),
                body,
                transport: None,
                model: model.map(str::to_owned),
                now: chrono::Utc::now().timestamp(),
            },
            error,
        };
    }
    transport_failure(error, model)
}

/// Read the reply's prelude. Returns the reply with every consumed byte replayed,
/// plus the credential revision the channel attached.
pub(crate) async fn inspect(
    wire: Wire,
    model: Option<&str>,
    mut response: reqwest::Response,
) -> Result<(reqwest::Response, Option<i64>), AttemptFailure> {
    let status = response.status();
    let mut headers = response.headers().clone();
    let revision = response
        .extensions()
        .get::<crate::SupplierAuthRevision>()
        .map(|r| r.0);
    if !status.is_success() {
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(error) => return Err(transport_failure(error.into(), model)),
        };
        let body = decode_error_body(&bytes);
        let mut error = UpstreamError::status_with_headers(
            status,
            String::from_utf8_lossy(&bytes).into_owned(),
            headers.clone(),
        );
        if let Some(revision) = revision {
            error = error.with_auth_revision(revision);
        }
        return Err(AttemptFailure {
            failure: UpstreamFailure {
                status: Some(status.as_u16()),
                headers,
                output_committed: generation_observed(&body),
                body,
                transport: None,
                model: model.map(str::to_owned),
                now: chrono::Utc::now().timestamp(),
            },
            error,
        });
    }
    let version = response.version();
    let extensions = std::mem::take(response.extensions_mut());
    let mut framing = None;
    let mut prefix = Vec::new();
    let mut parsed = 0;
    let mut scanned = 0;
    'read: while prefix.len() < crate::MAX_REQUEST_BYTES {
        if framing == Some(ResponseFormat::EventStream) && prefix.len() >= 256 * 1024 {
            break;
        }
        let chunk = match tokio::time::timeout(crate::DEFAULT_STREAM_IDLE_TIMEOUT, response.chunk())
            .await
        {
            Ok(Ok(chunk)) => chunk,
            Ok(Err(error)) => return Err(transport_failure(error.into(), model)),
            Err(_) => {
                return Err(transport_failure(UpstreamError::StreamIdleTimeout, model));
            }
        };
        let Some(chunk) = chunk else {
            if framing == Some(ResponseFormat::Json)
                && let Ok(value) = serde_json::from_slice::<Value>(
                    prefix.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&prefix),
                )
                && failure_event(wire, &value)
                && !generation_observed(&value)
            {
                return Err(event_failure(wire, value, &headers, revision, model));
            }
            if prefix.is_empty() {
                return Err(transport_failure(
                    UpstreamError::Stream("Upstream reply was empty".into()),
                    model,
                ));
            }
            break;
        };
        if prefix.len().saturating_add(chunk.len()) > crate::MAX_REQUEST_BYTES {
            return Err(transport_failure(
                UpstreamError::Stream("Upstream response exceeded the size limit".into()),
                model,
            ));
        }
        prefix.extend_from_slice(&chunk);
        if framing.is_none() {
            framing = ResponseFormat::from_prefix(&prefix);
            match framing {
                Some(ResponseFormat::EventStream) => {
                    headers.insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("text/event-stream"),
                    );
                }
                Some(ResponseFormat::Json) => {
                    headers.insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/json"),
                    );
                }
                Some(_) => break,
                None => continue,
            }
        }
        if framing == Some(ResponseFormat::Json) {
            continue;
        }
        loop {
            let remaining = &prefix[parsed..];
            let mut boundary = None;
            while scanned < prefix.len() {
                scanned += 1;
                let separator = if scanned >= parsed + 2 && &prefix[scanned - 2..scanned] == b"\n\n"
                {
                    2
                } else if scanned >= parsed + 4 && &prefix[scanned - 4..scanned] == b"\r\n\r\n" {
                    4
                } else {
                    continue;
                };
                boundary = Some((scanned - parsed - separator, separator));
                break;
            }
            let Some((end, separator)) = boundary else {
                break;
            };
            let frame = String::from_utf8_lossy(&remaining[..end]);
            let data = crate::sse_data(frame.strip_prefix('\u{feff}').unwrap_or(&frame));
            parsed += end + separator;
            if data.is_empty() || data.trim() == "[DONE]" {
                continue;
            }
            let Ok(mut value) = serde_json::from_str::<Value>(&data) else {
                break 'read;
            };
            if value.get("type").and_then(Value::as_str).is_none()
                && let Some(event) = frame
                    .lines()
                    .filter_map(|line| line.strip_prefix("event:"))
                    .next_back()
                && let Some(object) = value.as_object_mut()
            {
                object.insert("type".into(), event.trim().into());
            }
            if generation_observed(&value) {
                break 'read;
            }
            if failure_event(wire, &value) {
                return Err(event_failure(wire, value, &headers, revision, model));
            }
            if !is_prelude(wire, &value) {
                break 'read;
            }
        }
    }
    let stream = futures::stream::once(async move { Ok::<_, reqwest::Error>(Bytes::from(prefix)) })
        .chain(response.bytes_stream());
    let mut rebuilt = http::Response::new(reqwest::Body::wrap_stream(stream));
    *rebuilt.status_mut() = status;
    *rebuilt.version_mut() = version;
    *rebuilt.extensions_mut() = extensions;
    *rebuilt.headers_mut() = headers;
    if let Some(revision) = revision {
        rebuilt
            .extensions_mut()
            .insert(crate::SupplierAuthRevision(revision));
    }
    Ok((reqwest::Response::from(rebuilt), revision))
}

/// A failure event before output, as an attempt failure. The HTTP status was 2xx;
/// the event's own status, when it names one, is the status the channel judges.
fn event_failure(
    wire: Wire,
    value: Value,
    headers: &http::HeaderMap,
    revision: Option<i64>,
    model: Option<&str>,
) -> AttemptFailure {
    let status = crate::ResponseFailure::from_error(None, &value)
        .status
        .or_else(|| {
            // Google envelopes carry the HTTP status as the error's numeric code.
            (wire == Wire::Gemini)
                .then(|| crate::supply::error_object(&value)["code"].as_u64())
                .flatten()
                .and_then(|code| u16::try_from(code).ok())
                .filter(|code| (400..=599).contains(code))
        });
    let mut error = UpstreamError::status_with_headers(
        http::StatusCode::from_u16(status.unwrap_or(502)).unwrap_or(http::StatusCode::BAD_GATEWAY),
        value.to_string(),
        headers.clone(),
    );
    if let Some(revision) = revision {
        error = error.with_auth_revision(revision);
    }
    AttemptFailure {
        failure: UpstreamFailure {
            status,
            headers: headers.clone(),
            body: value,
            transport: None,
            model: model.map(str::to_owned),
            output_committed: false,
            now: chrono::Utc::now().timestamp(),
        },
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, sse: bool, text: &str) -> reqwest::Response {
        http::Response::builder()
            .status(status)
            .header(
                "content-type",
                if sse {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )
            .body(reqwest::Body::from(text.to_owned()))
            .unwrap()
            .into()
    }

    #[tokio::test]
    async fn prelude_failures_are_judged_but_output_is_never_replayed() {
        let prefix = "data: {\"type\":\"response.created\",\"response\":{\"id\":\"r\"}}\n\n";
        let rejection = "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"usage_limit_reached\",\"resets_at\":2000000000}}}\n\n";
        let failure = inspect(
            Wire::Codex,
            None,
            response(200, true, &format!("{prefix}{rejection}")),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(failure.failure.status, None);
        assert!(!failure.failure.output_committed);
        let output = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"visible\"}\n\n";
        let body = format!("{prefix}{output}{rejection}");
        let (replayed, _) = inspect(Wire::Codex, None, response(200, true, &body))
            .await
            .ok()
            .unwrap();
        assert_eq!(replayed.text().await.unwrap(), body);
        let http = inspect(
            Wire::Codex,
            None,
            response(429, false, "{\"error\":{\"code\":\"rate_limit_exceeded\"}}"),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(http.failure.status, Some(429));
        assert_eq!(http.failure.body["error"]["code"], "rate_limit_exceeded");
    }

    #[tokio::test]
    async fn gemini_chunks_with_parts_commit_and_errors_fail() {
        let empty = "data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[]}}]}}\n\n";
        let text = "data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hi\"}]}}]}}\n\n";
        let body = format!("{empty}{text}");
        let (replayed, _) = inspect(Wire::Gemini, None, response(200, true, &body))
            .await
            .ok()
            .unwrap();
        assert_eq!(replayed.text().await.unwrap(), body);
        let failed = format!(
            "{empty}data: {{\"error\":{{\"code\":429,\"status\":\"RESOURCE_EXHAUSTED\",\"message\":\"quota exceeded\"}}}}\n\n"
        );
        let failure = inspect(Wire::Gemini, Some("m"), response(200, true, &failed))
            .await
            .err()
            .unwrap();
        assert_eq!(failure.failure.status, Some(429));
    }
}

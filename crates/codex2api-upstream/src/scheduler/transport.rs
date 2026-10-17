//! Inspect uncommitted output and apply supplier-side recovery policy.
use super::{Result, SupplierContext};
use crate::{ResponseFormat, SupplierFailure, UpstreamError};
use bytes::Bytes;
use futures::StreamExt;

pub async fn observe_failure(
    storage: &codex2api_storage::Storage,
    id: &str,
    revision: Option<i64>,
    failure: &SupplierFailure,
) -> Result<()> {
    if let Some(revision) = revision {
        match failure {
            SupplierFailure::Authentication => storage.reject_supplier_auth(id, revision).await?,
            SupplierFailure::PaymentRequired { code } => {
                storage
                    .mark_supplier_payment_required(id, revision, code.as_deref(), None)
                    .await?
            }
            SupplierFailure::QuotaExhausted { until, code } => {
                storage
                    .exhaust_supplier_quota(id, revision, *until, code)
                    .await?
            }
        }
    }
    Ok(())
}

pub async fn recover_auth(
    storage: &codex2api_storage::Storage,
    upstream: &crate::UpstreamPool,
    ctx: &SupplierContext,
    observed_revision: Option<i64>,
) -> Result<Option<SupplierFailure>> {
    let snapshot = storage
        .supplier_auth_snapshot(&ctx.account.id)
        .await?
        .ok_or_else(|| UpstreamError::MissingAccessToken(ctx.account.id.clone()))?;
    if observed_revision.is_some_and(|revision| revision != snapshot.auth_revision) {
        return Ok(None);
    }
    if storage
        .supplier_health(&ctx.account.id)
        .await?
        .authentication_invalid
    {
        return Ok(Some(SupplierFailure::Authentication));
    }
    let token = snapshot.tokens.access_token.as_deref().unwrap_or("");
    match upstream.refresh_supplier_auth(&ctx.account, token).await {
        Ok(_) => Ok(None),
        Err(error) => {
            match error
                .supplier_failure_for(&ctx.account.provider_id, chrono::Utc::now().timestamp())
            {
                Some(failure) => Ok(Some(failure)),
                None => Err(error.into()),
            }
        }
    }
}

/// Buffer only the non-output prelude. Once output/tool execution is visible,
/// replay would duplicate side effects and is not a recoverable rejection.
pub fn prelude_event(value: &serde_json::Value) -> bool {
    matches!(
        value.get("type").and_then(serde_json::Value::as_str),
        Some("response.created" | "response.in_progress" | "response.queued" | "codex.rate_limits")
    )
}

/// Reported usage or generated output makes replay unsafe, including within an HTTP error body.
pub fn generation_observed(value: &serde_json::Value) -> bool {
    let response = value.get("response").unwrap_or(value);
    // A terminal failure can carry billable usage or partial output without an earlier
    // delta. Preserve it for settlement, even if its error is otherwise recoverable.
    response
        .get("usage")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|usage| !usage.is_empty())
        || response
            .get("message")
            .filter(|v| v.is_object())
            .is_some_and(generation_observed)
        || response
            .get("output")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|output| !output.is_empty())
        || response
            .get("data")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|data| !data.is_empty())
        || response
            .get("content")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|content| !content.is_empty())
        || response
            .get("choices")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|choices| {
                choices.iter().any(|choice| {
                    let message = choice
                        .get("delta")
                        .or_else(|| choice.get("message"))
                        .unwrap_or(choice);
                    message
                        .get("content")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|text| !text.is_empty())
                        || message
                            .get("reasoning_content")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|text| !text.is_empty())
                        || message.get("function_call").is_some_and(|v| !v.is_null())
                        || message
                            .get("tool_calls")
                            .and_then(serde_json::Value::as_array)
                            .is_some_and(|calls| !calls.is_empty())
                })
            })
}

pub async fn inspect_response(
    provider: &str,
    is_prelude: fn(&serde_json::Value) -> bool,
    mut response: reqwest::Response,
) -> std::result::Result<reqwest::Response, UpstreamError> {
    let status = response.status();
    let mut headers = response.headers().clone();
    if !status.is_success() {
        let revision = response
            .extensions()
            .get::<crate::SupplierAuthRevision>()
            .copied();
        let body = response.text().await?;
        let mut error = UpstreamError::status_with_headers(status, body, headers);
        if let Some(revision) = revision {
            error = error.with_auth_revision(revision.0);
        }
        return Err(error);
    }
    let version = response.version();
    let mut framing = None;
    let revision = response
        .extensions()
        .get::<crate::SupplierAuthRevision>()
        .copied();
    let extensions = std::mem::take(response.extensions_mut());
    let mut prefix = Vec::new();
    let mut parsed = 0;
    let mut scanned = 0;
    'read: while prefix.len() < crate::MAX_REQUEST_BYTES {
        if framing == Some(ResponseFormat::EventStream) && prefix.len() >= 256 * 1024 {
            break;
        }
        let chunk = tokio::time::timeout(crate::DEFAULT_STREAM_IDLE_TIMEOUT, response.chunk())
            .await
            .map_err(|_| UpstreamError::StreamIdleTimeout)??;
        let Some(chunk) = chunk else {
            if framing == Some(ResponseFormat::Json)
                && let Ok(value) = serde_json::from_slice::<serde_json::Value>(
                    prefix.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&prefix),
                )
            {
                reject_recoverable(provider, &value, &headers, revision)?;
            }
            break;
        };
        if prefix.len().saturating_add(chunk.len()) > crate::MAX_REQUEST_BYTES {
            return Err(UpstreamError::Stream(
                "Upstream response exceeded the size limit".into(),
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
            if data.is_empty() {
                continue;
            }
            let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&data) else {
                break 'read;
            };
            if value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_none()
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
            reject_recoverable(provider, &value, &headers, revision)?;
            if !is_prelude(&value) {
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
        rebuilt.extensions_mut().insert(revision);
    }
    Ok(reqwest::Response::from(rebuilt))
}

// The same output/usage guard and supplier classification apply to JSON and SSE.
fn reject_recoverable(
    provider: &str,
    value: &serde_json::Value,
    headers: &http::HeaderMap,
    revision: Option<crate::SupplierAuthRevision>,
) -> std::result::Result<(), UpstreamError> {
    if !generation_observed(value)
        && let Some(classified) = crate::classify_provider_failure(
            provider,
            None,
            value,
            headers,
            chrono::Utc::now().timestamp(),
        )
    {
        let failure = crate::ResponseFailure::from_error(None, value);
        let status = match classified {
            SupplierFailure::Authentication => 401,
            SupplierFailure::PaymentRequired { .. } => 402,
            SupplierFailure::QuotaExhausted { .. } => 429,
        };
        return Err(UpstreamError::Status {
            status: failure.status.unwrap_or(status),
            body: value.get("response").unwrap_or(value).to_string(),
            headers: headers.clone(),
            auth_revision: revision.map(|r| r.0),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn prelude_buffering_preserves_transport_token_timing() {
        let body = "data: {\"type\":\"response.created\"}\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"first\"}\n\n";
        let response: reqwest::Response = http::Response::builder()
            .header("content-type", "text/event-stream")
            .body(reqwest::Body::from(body))
            .unwrap()
            .into();
        let timing = crate::ResponseTiming::default();
        let response = inspect_response(
            "chatgpt",
            prelude_event,
            timing
                .clone()
                .wrap(response, crate::TokenProtocol::Responses),
        )
        .await
        .unwrap();
        let measured = timing.supplier_ttft_ms().unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(
            response
                .extensions()
                .get::<crate::ResponseTiming>()
                .unwrap()
                .supplier_ttft_ms(),
            Some(measured)
        );
        assert_eq!(response.text().await.unwrap(), body);
        assert_eq!(timing.supplier_ttft_ms(), Some(measured));
    }
    use serde_json::json;

    #[tokio::test]
    async fn failures_with_usage_or_output_remain_available_for_settlement() {
        for observed in [
            json!({"usage":{"input_tokens":100,"output_tokens":10}}),
            json!({"output":[{"type":"message","content":[{"type":"output_text","text":"partial"}]}]}),
        ] {
            let mut value = json!({"type":"response.failed","response":{"status":"failed","error":{"code":"usage_limit_reached","resets_at":2000000000}}});
            value["response"]
                .as_object_mut()
                .unwrap()
                .extend(observed.as_object().unwrap().clone());
            let body = format!(
                "data: {{\"type\":\"response.created\",\"response\":{{\"id\":\"r\"}}}}\n\ndata: {value}\n\n"
            );
            let mut response: reqwest::Response = http::Response::builder()
                .header("content-type", "text/event-stream")
                .body(reqwest::Body::from(body.clone()))
                .unwrap()
                .into();
            response
                .extensions_mut()
                .insert(crate::SupplierAuthRevision(7));
            let response = inspect_response("chatgpt", prelude_event, response)
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(
                response
                    .extensions()
                    .get::<crate::SupplierAuthRevision>()
                    .unwrap()
                    .0,
                7
            );
            assert_eq!(response.text().await.unwrap(), body);
        }
    }
}

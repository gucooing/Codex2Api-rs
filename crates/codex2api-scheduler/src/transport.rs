//! Inspect uncommitted output and apply supplier-side recovery policy.
use crate::{Result, SupplierContext};
use bytes::Bytes;
use codex2api_upstream::{SupplierFailure, UpstreamError};
use futures::StreamExt;
#[derive(Clone, Copy, Debug)]
pub struct FirstEventAt(pub std::time::Instant);

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
    upstream: &codex2api_upstream::UpstreamPool,
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
            .get("output")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|output| !output.is_empty())
        || response
            .get("data")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|data| !data.is_empty())
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
    let headers = response.headers().clone();
    if !status.is_success() {
        let body = response.text().await?;
        return Err(UpstreamError::status_with_headers(status, body, headers));
    }
    if !headers
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"))
    {
        return Ok(response);
    }
    let revision = response
        .extensions()
        .get::<codex2api_upstream::SupplierAuthRevision>()
        .copied();
    let mut prefix = Vec::new();
    let mut parsed = 0;
    let mut scanned = 0;
    let mut first_event = None;
    'read: while prefix.len() < 256 * 1024 {
        let chunk = tokio::time::timeout(
            codex2api_upstream::DEFAULT_STREAM_IDLE_TIMEOUT,
            response.chunk(),
        )
        .await
        .map_err(|_| UpstreamError::StreamIdleTimeout)??;
        let Some(chunk) = chunk else {
            break;
        };
        prefix.extend_from_slice(&chunk);
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
            let data = codex2api_upstream::sse_data(&frame);
            parsed += end + separator;
            if data.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&data) else {
                break 'read;
            };
            if value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|kind| kind.starts_with("response.") || kind == "error")
            {
                first_event.get_or_insert_with(std::time::Instant::now);
            }
            if generation_observed(&value) {
                break 'read;
            }
            if codex2api_upstream::classify_provider_failure(
                provider,
                None,
                &value,
                &headers,
                chrono::Utc::now().timestamp(),
            )
            .is_some()
            {
                let body = value.get("response").unwrap_or(&value).to_string();
                let failure = codex2api_upstream::ResponseFailure::from_error(None, &value);
                return Err(UpstreamError::Status {
                    status: failure.status.unwrap_or(429),
                    body,
                    headers,
                    auth_revision: revision.map(|revision| revision.0),
                });
            }
            if !is_prelude(&value) {
                break 'read;
            }
        }
    }
    let stream = futures::stream::once(async move { Ok::<_, reqwest::Error>(Bytes::from(prefix)) })
        .chain(response.bytes_stream());
    let mut rebuilt = http::Response::new(reqwest::Body::wrap_stream(stream));
    *rebuilt.status_mut() = status;
    *rebuilt.headers_mut() = headers;
    if let Some(at) = first_event {
        rebuilt.extensions_mut().insert(FirstEventAt(at));
    }
    if let Some(revision) = revision {
        rebuilt.extensions_mut().insert(revision);
    }
    Ok(reqwest::Response::from(rebuilt))
}

#[cfg(test)]
mod tests {
    use super::*;
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
                .insert(codex2api_upstream::SupplierAuthRevision(7));
            let response = inspect_response("chatgpt", prelude_event, response)
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(
                response
                    .extensions()
                    .get::<codex2api_upstream::SupplierAuthRevision>()
                    .unwrap()
                    .0,
                7
            );
            assert!(response.extensions().get::<FirstEventAt>().is_some());
            assert_eq!(response.text().await.unwrap(), body);
        }
    }
}

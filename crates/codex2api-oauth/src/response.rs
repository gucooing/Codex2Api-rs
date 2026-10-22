//! Shared HTTP response forwarding for every public API handler.
use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use codex2api_upstream::strip_hop_by_hop_headers;

/// An HTTP disconnect must not cancel a request while it awaits upstream headers.
/// Dropping the join handle detaches the worker; an undelivered response body is
/// then drained by the usage wrapper until its real upstream outcome is settled.
pub(crate) async fn complete_on_disconnect(
    work: impl std::future::Future<Output = crate::Result<Response>> + Send + 'static,
) -> crate::Result<Response> {
    tokio::spawn(codex2api_upstream::RequestFailureLog::inherit(work))
        .await
        .map_err(|error| {
            tracing::error!(%error, "upstream request worker failed");
            crate::OAuthError::internal("Upstream request worker failed.")
        })?
}

pub(crate) fn forward_response(status: StatusCode, mut headers: HeaderMap, body: Body) -> Response {
    strip_hop_by_hop_headers(&mut headers);
    // Upstream cookies belong to the isolated account's HTTP client.
    headers.remove(axum::http::header::SET_COOKIE);
    codex2api_upstream::convert::sanitize::headers(&mut headers);
    let is_json = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    if codex2api_upstream::ResponseFormat::from_headers(&headers)
        == codex2api_upstream::ResponseFormat::EventStream
    {
        // The account-local event transform may change bytes. Upstream lengths
        // and encodings cannot describe the resulting downstream stream.
        headers.remove(axum::http::header::CONTENT_LENGTH);
        headers.remove(axum::http::header::CONTENT_ENCODING);
    }
    let failed = status.is_client_error() || status.is_server_error();
    if failed {
        // A supplier's retry advice belongs to the executor's verdict, not the caller.
        for name in ["retry-after", "retry-after-ms", "x-should-retry"] {
            headers.remove(name);
        }
    }
    let public = StatusCode::from_u16(codex2api_upstream::convert::public_status(status.as_u16()))
        .unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
    let body = if failed || is_json {
        use futures::StreamExt;
        let encoded_headers = headers.clone();
        let stream = futures::stream::once(async move {
            // Drain through the usage wrapper so real settlement is retained, while
            // returning only the reviewed public error contract.
            let bytes = axum::body::to_bytes(body, codex2api_upstream::MAX_REQUEST_BYTES)
                .await
                .map_err(std::io::Error::other)?;
            let mut value =
                codex2api_upstream::decode_body(&bytes, &encoded_headers).unwrap_or_default();
            if failed {
                value = codex2api_upstream::convert::escaped_reply(
                    codex2api_upstream::inbound::Inbound::CodexClient,
                    status.as_u16(),
                    &value,
                )
                .1;
            } else {
                if value.is_null() {
                    return Err(std::io::Error::other("Invalid service response"));
                }
                codex2api_upstream::convert::sanitize::metadata(&mut value);
            }
            Ok::<_, std::io::Error>(axum::body::Bytes::from(value.to_string()))
        })
        .boxed();
        headers.remove(axum::http::header::CONTENT_LENGTH);
        headers.remove(axum::http::header::CONTENT_ENCODING);
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        Body::from_stream(stream)
    } else {
        body
    };
    let mut response = Response::new(body);
    *response.status_mut() = public;
    *response.headers_mut() = headers;
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Bytes, to_bytes};
    use axum::http::HeaderValue;
    use futures::StreamExt;
    use std::time::Duration;

    #[tokio::test]
    async fn disconnect_while_waiting_for_headers_does_not_cancel_the_worker() {
        let (started, start) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let (finished, finish) = tokio::sync::oneshot::channel();
        let request = tokio::spawn(complete_on_disconnect(async move {
            started.send(()).unwrap();
            released.await.unwrap();
            finished.send(()).unwrap();
            Ok(Response::new(Body::empty()))
        }));
        start.await.unwrap();
        request.abort();
        assert!(request.await.unwrap_err().is_cancelled());
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), finish)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn forwards_sse_bytes_before_upstream_finishes() {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("text/event-stream"),
        );
        headers.insert(
            "x-codex-turn-state",
            HeaderValue::from_static("sticky-route"),
        );
        headers.insert("x-request-id", HeaderValue::from_static("request"));
        headers.insert("x-codex-models-etag", HeaderValue::from_static("etag"));
        headers.insert("set-cookie", HeaderValue::from_static("account=private"));
        headers.insert("connection", HeaderValue::from_static("x-local"));
        headers.insert("x-local", HeaderValue::from_static("local"));
        let response = forward_response(StatusCode::OK, headers, Body::from_stream(stream));
        assert_eq!(response.headers()["x-codex-turn-state"], "sticky-route");
        assert_eq!(response.headers()["x-request-id"], "request");
        assert_eq!(response.headers()["x-codex-models-etag"], "etag");
        assert!(!response.headers().contains_key("set-cookie"));
        assert!(!response.headers().contains_key("connection"));
        assert!(!response.headers().contains_key("x-local"));
        let mut received = response.into_body().into_data_stream();
        for chunk in [
            ": keepalive\r\n\r\nevent: response.output_text.delta\r\ndata: {\"delta\":",
            "\"hello\"}\r\n\r\ndata: [DONE]\r\n\r\n",
        ] {
            tx.send(Ok::<_, std::io::Error>(Bytes::from_static(
                chunk.as_bytes(),
            )))
            .await
            .unwrap();
            let actual = tokio::time::timeout(Duration::from_secs(2), received.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(actual.as_ref(), chunk.as_bytes());
        }
        drop(tx);
        assert!(received.next().await.is_none());
    }

    #[tokio::test]
    async fn supplier_throttling_becomes_exhaustion_without_supplier_diagnostics() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("retry-after", "7".parse().unwrap());
        headers.insert("x-request-id", "request".parse().unwrap());
        headers.insert("x-openai-organization", "supplier-secret".parse().unwrap());
        headers.insert("x-debug-account", "supplier-secret".parse().unwrap());
        headers.insert("set-cookie", "session=supplier-secret".parse().unwrap());
        let response = forward_response(
            StatusCode::TOO_MANY_REQUESTS,
            headers,
            Body::from(
                r#"{"error":{"code":"rate_limit_exceeded","message":"supplier-secret","retry_after_ms":7000}}"#,
            ),
        );
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()["x-request-id"], "request");
        for name in [
            "retry-after",
            "x-debug-account",
            "set-cookie",
            "x-openai-organization",
        ] {
            assert!(!response.headers().contains_key(name), "{name}");
        }
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["error"]["code"], "no_available_supplier");
        assert!(!String::from_utf8_lossy(&bytes).contains("supplier-secret"));
    }

    #[tokio::test]
    async fn caller_faults_keep_their_status_and_reason() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        let response = forward_response(
            StatusCode::NOT_FOUND,
            headers,
            Body::from(r#"{"error":{"code":"conversation_not_found","message":"missing"}}"#),
        );
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["error"]["code"], "conversation_not_found");
    }
}

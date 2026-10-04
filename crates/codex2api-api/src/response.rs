//! Shared HTTP response forwarding for every public API handler.
use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use codex2api_upstream::strip_hop_by_hop_headers;

pub(crate) fn forward_response(status: StatusCode, mut headers: HeaderMap, body: Body) -> Response {
    strip_hop_by_hop_headers(&mut headers);
    // Upstream cookies belong to the isolated account's HTTP client.
    headers.remove(axum::http::header::SET_COOKIE);
    crate::public_output::headers(&mut headers);
    let is_json = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    let body = if status.is_client_error() || status.is_server_error() || is_json {
        use futures::StreamExt;
        let encoded_headers = headers.clone();
        let stream=futures::stream::once(async move {
            // Drain through the usage wrapper so real settlement is retained, while
            // returning only the reviewed public error contract.
            let bytes=axum::body::to_bytes(body,codex2api_upstream::MAX_REQUEST_BYTES).await.map_err(std::io::Error::other)?;
            let mut value=codex2api_upstream::decode_body(&bytes,&encoded_headers).unwrap_or_default();
            if !status.is_success() {
                value=serde_json::json!({"error":crate::public_output::error(value.get("error").unwrap_or(&value))});
            } else {
                if value.is_null() {return Err(std::io::Error::other("Invalid service response"));}
                crate::public_output::metadata(&mut value);
            }
            Ok::<_,std::io::Error>(axum::body::Bytes::from(value.to_string()))
        }).boxed();
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
    *response.status_mut() = status;
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
    async fn errors_keep_status_and_retry_but_not_supplier_diagnostics() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("retry-after", "7".parse().unwrap());
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
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()["retry-after"], "7");
        assert!(!response.headers().contains_key("x-debug-account"));
        assert!(!response.headers().contains_key("set-cookie"));
        assert!(!response.headers().contains_key("x-openai-organization"));
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["error"]["code"], "rate_limit_exceeded");
        assert_eq!(value["error"]["retry_after_ms"], 7000);
        assert!(!String::from_utf8_lossy(&bytes).contains("supplier-secret"));
    }
}

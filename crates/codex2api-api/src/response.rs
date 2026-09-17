//! Shared HTTP response forwarding for every public API handler.
use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use codex2api_upstream::strip_hop_by_hop_headers;

pub(crate) fn forward_response(status: StatusCode, mut headers: HeaderMap, body: Body) -> Response {
    strip_hop_by_hop_headers(&mut headers);
    // Upstream cookies belong to the isolated account's HTTP client.
    headers.remove(axum::http::header::SET_COOKIE);
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
    async fn preserves_upstream_status_encoding_and_error_body() {
        for (status, content_type, body) in [
            (
                StatusCode::OK,
                "application/json",
                b"{ \"id\": \"response\" }\n".as_slice(),
            ),
            (
                StatusCode::BAD_REQUEST,
                "application/json",
                b"{\"error\":{\"message\":\"exact upstream error\"}}".as_slice(),
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                "text/plain",
                b"upstream rate limit\r\n".as_slice(),
            ),
            (
                StatusCode::BAD_GATEWAY,
                "application/octet-stream",
                b"\x28\xb5\x2f\xfd\x00\xff".as_slice(),
            ),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("content-type", HeaderValue::from_static(content_type));
            headers.insert("retry-after", HeaderValue::from_static("7"));
            headers.insert("content-length", HeaderValue::from(body.len()));
            headers.insert("content-encoding", HeaderValue::from_static("zstd"));
            let response = forward_response(status, headers.clone(), Body::from(body));
            assert_eq!(response.status(), status);
            assert_eq!(response.headers(), &headers);
            assert_eq!(
                to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .as_ref(),
                body
            );
        }
    }
}

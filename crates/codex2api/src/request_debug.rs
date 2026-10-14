use axum::{body::Body, extract::Request, middleware::Next, response::Response};
use codex2api_upstream::request_debug::{self, RequestTrace};
use http_body_util::BodyExt;

pub(crate) async fn trace(request: Request, next: Next) -> Response {
    if !request_debug::enabled() || !inference_path(request.uri().path()) {
        return next.run(request).await;
    }
    let trace = RequestTrace::new("http", request.uri().path());
    let (parts, body) = request.into_parts();
    let input = trace.clone();
    let body = Body::new(body.map_frame(move |frame| {
        if let Some(bytes) = frame.data_ref() {
            input.input(bytes);
        }
        frame
    }));
    let response = next.run(Request::from_parts(parts, body)).await;
    trace.response_status(response.status().as_u16());
    response.map(|body| {
        Body::new(body.map_frame(move |frame| {
            if let Some(bytes) = frame.data_ref() {
                trace.output(bytes);
            }
            frame
        }))
    })
}

fn inference_path(path: &str) -> bool {
    let relative = ["/v1/", "/backend-api/codex/", "/grok/v1/"]
        .into_iter()
        .find_map(|prefix| path.strip_prefix(prefix));
    relative.is_some_and(|path| {
        matches!(
            path,
            "responses"
                | "responses/compact"
                | "responses/input_tokens"
                | "chat/completions"
                | "messages"
                | "images/generations"
                | "images/edits"
                | "alpha/search"
                | "guardian"
                | "guardian-classifier"
                | "memories/trace_summarize"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Bytes, to_bytes},
        routing::post,
    };
    use std::{
        io::Write,
        sync::{Arc, Mutex},
    };
    use tower::ServiceExt;
    use tracing::instrument::WithSubscriber;

    #[derive(Clone)]
    struct Console(Arc<Mutex<Vec<u8>>>);
    impl Write for Console {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn debug_logs_full_bodies_without_changing_streams_or_logging_auth() {
        for (path, level, logged) in [
            ("/v1/responses", tracing::Level::DEBUG, true),
            ("/v1/responses", tracing::Level::INFO, false),
            ("/oauth/token", tracing::Level::DEBUG, false),
        ] {
            let console = Console(Default::default());
            let capture = console.clone();
            let subscriber = tracing_subscriber::fmt()
                .without_time()
                .with_ansi(false)
                .with_max_level(level)
                .with_writer(move || capture.clone())
                .finish();
            let payload = format!("{}-UNTRUNCATED-END", "full-input-output".repeat(10_000));
            let expected = payload.clone();
            let app = Router::new()
                .route(
                    path,
                    post(|body: Bytes| async move {
                        Body::from_stream(futures::stream::iter(vec![
                            Ok::<_, std::io::Error>(body),
                            Ok(Bytes::from_static(b"\xf0\x9f")),
                            Ok(Bytes::from_static(b"\x99\x82")),
                        ]))
                    }),
                )
                .layer(axum::middleware::from_fn(trace));
            async move {
                let response = app
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(path)
                            .header("authorization", "Bearer must-not-log-this-header")
                            .body(Body::from(payload))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                assert_eq!(&body[..expected.len()], expected.as_bytes());
                assert_eq!(&body[expected.len()..], "🙂".as_bytes());
            }
            .with_subscriber(subscriber)
            .await;
            let output = String::from_utf8(console.0.lock().unwrap().clone()).unwrap();
            if logged {
                assert_eq!(output.matches("-UNTRUNCATED-END").count(), 2);
                assert!(output.contains("direction=output"));
                assert!(output.contains("encoding=base64"));
            } else {
                assert!(output.is_empty());
            }
            assert!(!output.contains("must-not-log-this-header"));
        }
    }
}

use super::*;
fn body(stream: bool) -> Value {
    json!({"model":"custom-model","max_tokens":1024,"stream":stream,"system":"Be precise","messages":[{"role":"user","content":"Hello"}],"metadata":{"user_id":"private-consumer"}})
}
#[tokio::test]
async fn messages_use_each_suppliers_responses_and_settle_before_completion() {
    for (provider, path) in [
        ("chatgpt", "/backend-api/codex/responses"),
        ("grok", "/v1/responses"),
    ] {
        for stream in [false, true] {
            let f = fixture(provider).await;
            let mut req = request("/v1/messages", &f.token, body(stream));
            req.headers_mut().remove("authorization");
            req.headers_mut()
                .insert("x-api-key", f.token.parse().unwrap());
            let response = f.router.clone().oneshot(req).await.unwrap();
            let status = response.status();
            let text = String::from_utf8(
                to_bytes(response.into_body(), 1024 * 1024)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap();
            assert_eq!(status, StatusCode::OK, "{provider} {stream}: {text}");
            assert!(text.contains("Hello"), "{text}");
            assert!(!text.contains("private-consumer"));
            assert!(!text.contains("private-supplier"));
            if stream {
                assert!(text.contains("event: message_start"), "{text}");
                assert!(text.contains("event: message_delta"));
                assert!(text.contains("event: message_stop"));
                assert!(!text.contains("[DONE]"));
            } else {
                let value: Value = serde_json::from_str(&text).unwrap();
                assert_eq!(value["type"], "message");
                assert_eq!(value["usage"]["input_tokens"], 8000);
                assert_eq!(value["usage"]["cache_read_input_tokens"], 2000);
                assert_eq!(value["stop_reason"], "end_turn");
            }
            let rows:Vec<(String,String,Option<i64>,Option<i64>,Option<i64>,Option<i64>)>=sqlx::query_as("SELECT endpoint,status,input_tokens,cached_tokens,output_tokens,cost_nano_usd FROM usage_records").fetch_all(f.storage.pool()).await.unwrap();
            assert_eq!(
                rows,
                vec![(
                    "/v1/messages".into(),
                    "completed".into(),
                    Some(10000),
                    Some(2000),
                    Some(5000),
                    Some(27_300_000)
                )]
            );
            assert_eq!(
                f.storage.user("user").await.unwrap().unwrap().wallet_cents,
                98
            );
            let public = f
                .storage
                .user_store()
                .usage("user", &codex2api_storage::UserUsageFilter::default())
                .await
                .unwrap();
            assert_eq!(public["items"][0]["endpoint"], "/v1/messages");
            assert!(public["items"][0].get("upstream_endpoint").is_none());
            assert!(public["items"][0].get("account_id").is_none());
            let endpoint: Option<String> =
                sqlx::query_scalar("SELECT upstream_endpoint FROM usage_records")
                    .fetch_one(f.storage.pool())
                    .await
                    .unwrap();
            assert_eq!(endpoint.as_deref(), Some(path));
            let calls = f.mock.requests.lock().unwrap();
            assert_eq!(calls[0].0, path);
            assert!(!calls[0].1.contains_key("x-api-key"));
            assert!(calls[0].2.get("metadata").is_none());
            assert_eq!(calls[0].2["stream"], true);
        }
    }
}
#[tokio::test]
async fn messages_errors_reject_credentials_without_fallback_and_keep_retry_headers() {
    let f = fixture("chatgpt").await;
    let mut req = request("/v1/messages", "bad", body(false));
    req.headers_mut()
        .insert("x-api-key", f.token.parse().unwrap());
    let response = f.router.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json_body(response).await["type"], "error");
    assert!(f.mock.requests.lock().unwrap().is_empty());
    *f.mock.status.lock().unwrap() = Some(429);
    let response = f
        .router
        .clone()
        .oneshot(request("/v1/messages", &f.token, body(false)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "7");
    let value = json_body(response).await;
    assert_eq!(value["type"], "error");
    assert_eq!(value["error"]["type"], "rate_limit_error");
    assert_eq!(f.mock.requests.lock().unwrap().len(), 1);
    assert!(
        f.storage
            .supplier_health("supplier")
            .await
            .unwrap()
            .account_block()
            .and_then(|block| block.until_at)
            .is_none()
    );
}
#[tokio::test]
async fn messages_failover_rebuilds_the_request_for_the_next_account() {
    let f = grok_scheduling::pool().await;
    f.mock.grok_rejections.lock().unwrap().insert(
        "supplier".into(),
        (
            429,
            json!({"code":"subscription:free-usage-exhausted","error":"quota exhausted"})
                .to_string(),
        ),
    );
    let response = f
        .router
        .clone()
        .oneshot(request("/v1/messages", &f.token, body(false)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["content"][0]["text"], "Hello");
    let calls = f.mock.requests.lock().unwrap();
    assert_eq!(
        calls.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
        ["/v1/responses", "/v1/responses"]
    );
    assert_eq!(calls[0].1["x-grok-req-id"], calls[1].1["x-grok-req-id"]);
    assert_eq!(calls[0].2, calls[1].2);
    drop(calls);
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT account_id,status FROM usage_records")
        .fetch_all(f.storage.pool())
        .await
        .unwrap();
    assert_eq!(rows, [("supplier-two".into(), "completed".into())]);
}
#[tokio::test]
async fn messages_stream_truncation_never_emits_success() {
    let f = fixture("chatgpt").await;
    f.mock.truncate.store(true, Ordering::Relaxed);
    let response = f
        .router
        .clone()
        .oneshot(request("/v1/messages", &f.token, body(true)))
        .await
        .unwrap();
    let text = String::from_utf8(
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(text.contains("event: error"));
    assert!(!text.contains("event: message_stop"));
    let status: String = sqlx::query_scalar("SELECT status FROM usage_records")
        .fetch_one(f.storage.pool())
        .await
        .unwrap();
    assert_eq!(status, "failed");
}

#[tokio::test]
async fn grok_serves_every_inbound_format_through_responses() {
    for (path, done, kind) in [
        ("/v1/responses", "response.completed", "response"),
        ("/v1/chat/completions", "[DONE]", "chat.completion"),
        ("/v1/messages", "message_stop", "message"),
    ] {
        for stream in [false, true] {
            let f = fixture("grok").await;
            let body = match path {
                "/v1/responses" => {
                    json!({"model":"custom-model","input":"Hello","max_output_tokens":1024,"stream":stream})
                }
                "/v1/chat/completions" => {
                    json!({"model":"custom-model","messages":[{"role":"user","content":"Hello"}],"max_completion_tokens":1024,"stream":stream})
                }
                _ => body(stream),
            };
            let response = f
                .router
                .clone()
                .oneshot(request(path, &f.token, body))
                .await
                .unwrap();
            let status = response.status();
            let text = String::from_utf8(
                to_bytes(response.into_body(), 1024 * 1024)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap();
            assert_eq!(status, StatusCode::OK, "{path} stream={stream}: {text}");
            assert!(text.contains("Hello"), "{text}");
            assert!(!text.contains("event: error"), "{text}");
            if stream {
                assert!(text.contains(done), "{path}: {text}");
            } else {
                let value: Value = serde_json::from_str(&text).unwrap();
                let actual = value.get("object").unwrap_or(&value["type"]);
                assert_eq!(actual, kind, "{text}");
            }
            let calls = f.mock.requests.lock().unwrap();
            assert_eq!(calls[0].0, "/v1/responses");
            assert_eq!(calls[0].2["max_output_tokens"], 1024);
            assert_eq!(calls[0].2["store"], false);
            drop(calls);
            let charge: i64 = sqlx::query_scalar("SELECT cost_nano_usd FROM usage_records")
                .fetch_one(f.storage.pool())
                .await
                .unwrap();
            assert_eq!(charge, 27_300_000);
        }
    }
}

#[tokio::test]
async fn messages_failure_usage_is_charged_once_without_replay() {
    let f = grok_scheduling::pool().await;
    f.mock.grok_rejections.lock().unwrap().insert("supplier".into(),(429,json!({"error":{"code":"subscription:free-usage-exhausted","message":"quota"},"model":"custom-model","usage":{"input_tokens":10000,"input_tokens_details":{"cached_tokens":2000},"output_tokens":5000}}).to_string()));
    let response = f
        .router
        .clone()
        .oneshot(request("/v1/messages", &f.token, body(false)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let _ = json_body(response).await;
    assert_eq!(f.mock.requests.lock().unwrap().len(), 1);
    let row: (String, Option<i64>, Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT status,input_tokens,cached_tokens,cost_nano_usd FROM usage_records")
            .fetch_one(f.storage.pool())
            .await
            .unwrap();
    assert_eq!(
        row,
        ("failed".into(), Some(10000), Some(2000), Some(27_300_000))
    );
}

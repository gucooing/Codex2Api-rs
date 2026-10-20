//! Full executor/ledger regressions for the API-key wires. Only upstream HTTP is mocked.
use super::*;
use std::collections::VecDeque;

struct WireMock {
    replies: Mutex<VecDeque<(u16, String)>>,
    calls: std::sync::atomic::AtomicUsize,
    paths: Mutex<Vec<String>>,
    change_group: Option<(Storage, ApiGroup)>,
}
impl codex2api_upstream::HttpTransport for WireMock {
    fn send(&self, request: reqwest::RequestBuilder) -> codex2api_upstream::HttpFuture {
        let request = request.build();
        if let Ok(request) = &request {
            self.paths
                .lock()
                .unwrap()
                .push(request.url().path().to_owned());
        }
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected retry");
        let first = self.calls.fetch_add(1, Ordering::Relaxed) == 0;
        let change = self.change_group.clone().filter(|_| first);
        Box::pin(async move {
            let request = request?;
            assert!(!request.headers().contains_key("chatgpt-account-id"));
            if let Some((storage, mut group)) = change {
                group.enabled = false;
                storage.save_api_group(&group, Some(group.revision)).await?;
            }
            Ok(http::Response::builder()
                .status(reply.0)
                .header(
                    "content-type",
                    if reply.1.starts_with("data:") {
                        "text/event-stream"
                    } else {
                        "application/json"
                    },
                )
                .body(reqwest::Body::from(reply.1))
                .unwrap()
                .into())
        })
    }
}

fn wire_stream(wire: &str) -> String {
    let events = if wire == "messages" {
        vec![
            json!({"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","model":"custom-model","content":[],"usage":{"input_tokens":10000,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5000}}),
            json!({"type":"message_stop"}),
        ]
    } else {
        vec![
            json!({"id":"chat_1","model":"custom-model","choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"},"finish_reason":null}]}),
            json!({"id":"chat_1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}),
            json!({"id":"chat_1","choices":[],"usage":{"prompt_tokens":10000,"completion_tokens":5000}}),
        ]
    };
    events.iter().map(|v| format!("data: {v}\n\n")).collect()
}

async fn api_fixture(
    wire: &str,
    replies: Vec<(u16, String)>,
    revoke: bool,
) -> (Fixture, Arc<WireMock>) {
    let mut f = fixture("chatgpt").await;
    let auth = codex2api_suppliers::api::ApiAuthService::new(Some(f.storage.clone()));
    let (account, _) = auth
        .create(
            "fixture",
            codex2api_suppliers::api::ApiCredentials {
                api_key: "fixture-key".into(),
                base_url: "https://example.test/gateway/v1".into(),
                wire: wire.into(),
                auth_scheme: "bearer".into(),
                retry_statuses: vec![503],
                retry_count: 2,
            },
            None,
            &["custom-model".into()],
        )
        .await
        .unwrap();
    f.storage
        .save_supplier_tag("api-tag", "api", "API")
        .await
        .unwrap();
    f.storage
        .edit_supplier_tags(&[account.id], &["api-tag".into()], false)
        .await
        .unwrap();
    let mut group = f
        .storage
        .authenticate_api_key(&f.token)
        .await
        .unwrap()
        .unwrap()
        .group;
    group.tag_ids = vec!["api-tag".into()];
    f.storage
        .save_api_group(&group, Some(group.revision))
        .await
        .unwrap();
    let group = f
        .storage
        .authenticate_api_key(&f.token)
        .await
        .unwrap()
        .unwrap()
        .group;
    let mock = Arc::new(WireMock {
        replies: Mutex::new(replies.into()),
        calls: Default::default(),
        paths: Default::default(),
        change_group: revoke.then(|| (f.storage.clone(), group)),
    });
    let accounts = codex2api_suppliers::SupplierAccountStore::open(f.storage.clone());
    let pool = codex2api_upstream::UpstreamPool::new(
        codex2api_suppliers::AuthService::new(accounts).unwrap(),
    )
    .with_transport(mock.clone());
    f.router = codex2api_api::router(codex2api_api::ApiState::new(f.storage.clone(), pool));
    (f, mock)
}

struct StagedStream {
    first: String,
    rest: Mutex<Option<tokio::sync::mpsc::Receiver<std::io::Result<Bytes>>>>,
}
impl codex2api_upstream::HttpTransport for StagedStream {
    fn send(&self, request: reqwest::RequestBuilder) -> codex2api_upstream::HttpFuture {
        let request = request.build();
        if let Ok(request) = &request
            && request.method() == http::Method::GET
        {
            // ChatGPT performs its workspace discovery before generation.
            let workspace = request.headers()["chatgpt-account-id"]
                .to_str()
                .unwrap()
                .to_owned();
            return Box::pin(async move {
                Ok(http::Response::builder().header("content-type","application/json")
                    .body(reqwest::Body::from(json!({"accounts":[{"id":workspace,
                        "workspace_backend_origin":"NO_CONSTRAINT","account_routing_override":"NO_CONSTRAINT"}]}).to_string()))
                    .unwrap().into())
            });
        }
        // Split frames and UTF-8 across transport chunks, then leave the wire open.
        let first: Vec<_> = self
            .first
            .as_bytes()
            .chunks(7)
            .map(|bytes| Ok::<_, std::io::Error>(Bytes::copy_from_slice(bytes)))
            .collect();
        let rest = self
            .rest
            .lock()
            .unwrap()
            .take()
            .expect("unexpected supplier replay");
        Box::pin(async move {
            request?;
            let stream = futures::stream::iter(first)
                .chain(tokio_stream::wrappers::ReceiverStream::new(rest));
            Ok(http::Response::builder()
                .header("content-type", "text/event-stream")
                .body(reqwest::Body::wrap_stream(stream))
                .unwrap()
                .into())
        })
    }
}

fn staged_events(wire: &str) -> [Vec<Value>; 3] {
    match wire {
        "gemini" => [
            vec![
                json!({"response":{"candidates":[{"content":{"parts":[{"thought":true,"text":"思考 phase-one"}]}}]}}),
            ],
            vec![json!({"response":{"candidates":[{"content":{"parts":[
                {"text":"正文 phase-two"},
                {"functionCall":{"id":"call_1","name":"lookup","args":{"city":"Taipei"}},"thoughtSignature":"call-signature"}
            ]},"finishReason":"STOP"}]}})],
            vec![
                json!({"response":{"usageMetadata":{"promptTokenCount":10000,"candidatesTokenCount":4000,"thoughtsTokenCount":1000}}}),
            ],
        ],
        "chat_completions" => [
            vec![
                json!({"id":"chat_1","model":"custom-model","choices":[{"index":0,"delta":{"reasoning":"思考 phase-one"},"finish_reason":null}]}),
            ],
            vec![
                json!({"id":"chat_1","choices":[{"index":0,"delta":{"content":"正文 phase-two","tool_calls":[
                {"index":0,"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"city\":\"Taipei\"}"}}
            ]},"finish_reason":"tool_calls"}]}),
            ],
            vec![
                json!({"id":"chat_1","choices":[],"usage":{"prompt_tokens":10000,"completion_tokens":5000,"completion_tokens_details":{"reasoning_tokens":1000}}}),
            ],
        ],
        "messages" => [
            vec![
                json!({"type":"message_start","message":{"id":"msg_1","model":"custom-model","role":"assistant","content":[],"usage":{"input_tokens":10000,"output_tokens":0}}}),
                json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}),
                json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"思考 phase-one"}}),
            ],
            vec![
                json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"thinking-signature"}}),
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}),
                json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"正文 phase-two"}}),
                json!({"type":"content_block_stop","index":1}),
                json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call_1","name":"lookup","input":{}}}),
                json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"city\":\"Taipei\"}"}}),
                json!({"type":"content_block_stop","index":2}),
            ],
            vec![
                json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5000}}),
                json!({"type":"message_stop"}),
            ],
        ],
        _ => {
            let reasoning = json!({"id":"rs_1","type":"reasoning","summary":[{"type":"summary_text","text":"思考 phase-one"}],"encrypted_content":"thinking-signature"});
            let message = json!({"id":"msg_1","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"正文 phase-two","annotations":[]}]});
            let tool = json!({"id":"fc_1","type":"function_call","call_id":"call_1","name":"lookup","arguments":"{\"city\":\"Taipei\"}","status":"completed"});
            [
                vec![
                    json!({"type":"response.created","response":{"id":"resp_1","model":"custom-model","created_at":100,"status":"in_progress"}}),
                    json!({"type":"response.output_item.added","output_index":0,"item":{"id":"rs_1","type":"reasoning","summary":[]}}),
                    json!({"type":"response.reasoning_summary_part.added","output_index":0,"item_id":"rs_1","summary_index":0,"part":{"type":"summary_text","text":""}}),
                    json!({"type":"response.reasoning_summary_text.delta","output_index":0,"item_id":"rs_1","summary_index":0,"delta":"思考 phase-one"}),
                ],
                vec![
                    json!({"type":"response.output_item.done","output_index":0,"item":reasoning}),
                    json!({"type":"response.output_item.added","output_index":1,"item":{"id":"msg_1","type":"message","role":"assistant","content":[]}}),
                    json!({"type":"response.content_part.added","output_index":1,"item_id":"msg_1","content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
                    json!({"type":"response.output_text.delta","output_index":1,"item_id":"msg_1","content_index":0,"delta":"正文 phase-two"}),
                    json!({"type":"response.output_item.done","output_index":1,"item":message}),
                    json!({"type":"response.output_item.added","output_index":2,"item":{"id":"fc_1","type":"function_call","call_id":"call_1","name":"lookup","arguments":""}}),
                    json!({"type":"response.function_call_arguments.delta","output_index":2,"item_id":"fc_1","delta":"{\"city\":\"Taipei\"}"}),
                    json!({"type":"response.output_item.done","output_index":2,"item":tool}),
                ],
                vec![
                    json!({"type":"response.completed","response":{"id":"resp_1","model":"custom-model","created_at":100,"status":"completed","output":[reasoning,message,tool],
                    "usage":{"input_tokens":10000,"output_tokens":5000,"output_tokens_details":{"reasoning_tokens":1000}}}}),
                ],
            ]
        }
    }
}

fn sse(events: &[Value]) -> String {
    events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect()
}

#[tokio::test]
async fn every_wire_delivers_thoughts_text_and_tools_before_the_upstream_closes() {
    use std::time::Duration;
    for wire in [
        "gemini",
        "chat_completions",
        "messages",
        "openai_responses",
        "chatgpt",
        "grok",
    ] {
        for (path, body, terminal) in [
            (
                "/v1/responses",
                json!({"model":"custom-model","input":"hello","stream":true}),
                "response.completed",
            ),
            (
                "/v1/chat/completions",
                json!({"model":"custom-model","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_usage":true}}),
                "[DONE]",
            ),
            (
                "/v1/messages",
                json!({"model":"custom-model","max_tokens":100,"messages":[{"role":"user","content":"hello"}],"stream":true}),
                "message_stop",
            ),
        ] {
            let mut f = match wire {
                "gemini" => fixture("antigravity").await,
                "chatgpt" | "grok" => fixture(wire).await,
                _ => api_fixture(wire, vec![], false).await.0,
            };
            let stages = staged_events(wire);
            let (sender, receiver) = tokio::sync::mpsc::channel(2);
            let mock = Arc::new(StagedStream {
                first: sse(&stages[0]),
                rest: Mutex::new(Some(receiver)),
            });
            let accounts = codex2api_suppliers::SupplierAccountStore::open(f.storage.clone());
            let pool = codex2api_upstream::UpstreamPool::new(
                codex2api_suppliers::AuthService::new(accounts).unwrap(),
            )
            .with_transport(mock);
            f.router = codex2api_api::router(codex2api_api::ApiState::new(f.storage.clone(), pool));
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                f.router.clone().oneshot(request(path, &f.token, body)),
            )
            .await
            .unwrap_or_else(|_| panic!("{wire} {path}: headers waited for upstream completion"))
            .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{wire} {path}");
            let mut body = response.into_body().into_data_stream();
            let mut received = String::new();
            for (phase, needle) in [(0, "phase-one"), (1, "Taipei")] {
                if phase == 1 {
                    sender.send(Ok(Bytes::from(sse(&stages[1])))).await.unwrap();
                }
                tokio::time::timeout(Duration::from_secs(5), async {
                    while !received.contains(needle) {
                        let next = body
                            .next()
                            .await
                            .expect("stream ended before the next phase")
                            .unwrap();
                        received.push_str(std::str::from_utf8(&next).unwrap());
                    }
                })
                .await
                .unwrap_or_else(|_| {
                    panic!("{wire} {path}: {needle} waited for upstream completion: {received}")
                });
                assert!(
                    !received.contains(terminal),
                    "premature completion: {received}"
                );
            }
            assert!(received.contains("phase-two"));
            let status: String = sqlx::query_scalar("SELECT status FROM usage_records")
                .fetch_one(f.storage.pool())
                .await
                .unwrap();
            assert_eq!(status, "in_progress");
            sender.send(Ok(Bytes::from(sse(&stages[2])))).await.unwrap();
            drop(sender);
            tokio::time::timeout(Duration::from_secs(5), async {
                while let Some(next) = body.next().await {
                    received.push_str(std::str::from_utf8(&next.unwrap()).unwrap());
                }
            })
            .await
            .expect("stream did not complete");
            assert!(received.contains(terminal), "{wire} {path}: {received}");
            assert!(!received.contains("upstream_protocol_error"), "{received}");
            let row: (String, Option<i64>, Option<i64>, i64) = sqlx::query_as(
                "SELECT status,input_tokens,output_tokens,wallet_debit_cents FROM usage_records",
            )
            .fetch_one(f.storage.pool())
            .await
            .unwrap();
            assert_eq!(
                (&row.0, row.1, row.2),
                (&"completed".to_owned(), Some(10000), Some(5000))
            );
            assert!(row.3 > 0);
        }
    }
}

#[tokio::test]
async fn api_wire_tail_is_delivered_and_wallet_uses_final_usage() {
    for wire in ["chat_completions", "messages"] {
        for (path, body, terminal) in [
            (
                "/v1/responses",
                json!({"model":"custom-model","input":"hi"}),
                "response.completed",
            ),
            (
                "/v1/chat/completions",
                json!({"model":"custom-model","messages":[{"role":"user","content":"hi"}]}),
                "[DONE]",
            ),
            (
                "/v1/messages",
                json!({"model":"custom-model","max_tokens":100,"messages":[{"role":"user","content":"hi"}]}),
                "message_stop",
            ),
        ] {
            for streaming in [false, true] {
                let (f, mock) = api_fixture(wire, vec![(200, wire_stream(wire))], false).await;
                let mut body = body.clone();
                body["stream"] = streaming.into();
                let response = f
                    .router
                    .clone()
                    .oneshot(request(path, &f.token, body))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK, "{wire} {path}");
                let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
                let text = String::from_utf8(bytes.to_vec()).unwrap();
                assert!(text.contains("Hello"), "{text}");
                if streaming {
                    assert!(text.contains(terminal), "{text}");
                }
                let row: (String, Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as(
                    "SELECT status,input_tokens,output_tokens,cost_nano_usd FROM usage_records",
                )
                .fetch_one(f.storage.pool())
                .await
                .unwrap();
                assert_eq!(
                    row,
                    (
                        "completed".into(),
                        Some(10000),
                        Some(5000),
                        Some(30_000_000)
                    )
                );
                assert_eq!(mock.calls.load(Ordering::Relaxed), 1);
                let endpoint: Option<String> =
                    sqlx::query_scalar("SELECT upstream_endpoint FROM usage_records")
                        .fetch_one(f.storage.pool())
                        .await
                        .unwrap();
                let suffix = if wire == "messages" {
                    "messages"
                } else {
                    "chat/completions"
                };
                let expected = format!("/gateway/v1/{suffix}");
                assert_eq!(endpoint.as_deref(), Some(expected.as_str()));
                assert_eq!(*mock.paths.lock().unwrap(), [expected]);
            }
        }
    }
}

#[tokio::test]
async fn retry_is_recorded_once_and_revalidates_the_consumer() {
    for revoked in [false, true] {
        let (f, mock) = api_fixture(
            "chat_completions",
            vec![
                (
                    503,
                    json!({"error":{"type":"server_error","message":"temporarily unavailable"}})
                        .to_string(),
                ),
                (200, wire_stream("chat_completions")),
            ],
            revoked,
        )
        .await;
        let response = f
            .router
            .clone()
            .oneshot(request(
                "/v1/chat/completions",
                &f.token,
                json!({"model":"custom-model","messages":[{"role":"user","content":"hi"}]}),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if revoked {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::OK
            }
        );
        to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(
            mock.calls.load(Ordering::Relaxed),
            if revoked { 1 } else { 2 }
        );
        let recovery: Vec<String> = sqlx::query_scalar("SELECT recovery FROM usage_attempts")
            .fetch_all(f.storage.pool())
            .await
            .unwrap();
        assert_eq!(recovery, ["retry"]);
        let attempt_endpoint: Option<String> =
            sqlx::query_scalar("SELECT upstream_endpoint FROM usage_attempts")
                .fetch_one(f.storage.pool())
                .await
                .unwrap();
        assert_eq!(
            attempt_endpoint.as_deref(),
            Some("/gateway/v1/chat/completions")
        );
        let row: (String, Option<i64>) =
            sqlx::query_as("SELECT status,output_tokens FROM usage_records")
                .fetch_one(f.storage.pool())
                .await
                .unwrap();
        assert_eq!(
            row,
            if revoked {
                ("failed".into(), None)
            } else {
                ("completed".into(), Some(5000))
            }
        );
    }
}

#[tokio::test]
async fn configured_but_blocked_model_returns_unavailable_not_missing() {
    let f = fixture("chatgpt").await;
    f.storage
        .add_supplier_model("supplier", "other-model")
        .await
        .unwrap();
    let revision = f
        .storage
        .supplier_auth_revision("supplier")
        .await
        .unwrap()
        .unwrap();
    f.storage
        .block_supplier(
            "supplier",
            revision,
            &codex2api_storage::BlockRequest {
                model: Some("custom-model".into()),
                kind: codex2api_storage::BlockKind::Quota,
                until_at: Some(chrono::Utc::now().timestamp() + 600),
                code: None,
                message: None,
            },
        )
        .await
        .unwrap();
    let response = f
        .router
        .clone()
        .oneshot(request(
            "/v1/responses",
            &f.token,
            json!({"model":"custom-model","input":"hi"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "no_available_supplier"
    );
    assert!(f.mock.requests.lock().unwrap().is_empty());
    let endpoint: Option<String> =
        sqlx::query_scalar("SELECT upstream_endpoint FROM usage_records")
            .fetch_one(f.storage.pool())
            .await
            .unwrap();
    assert!(endpoint.is_none(), "no supplier request was sent");
    f.storage
        .remove_supplier_model("supplier", "custom-model")
        .await
        .unwrap();
    let response = f
        .router
        .clone()
        .oneshot(request(
            "/v1/responses",
            &f.token,
            json!({"model":"custom-model","input":"hi"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "model_not_found"
    );
}

async fn failover_fixture(first: &str, second: &str) -> Fixture {
    let f = fixture(first).await;
    add_supplier(&f.storage, second, "supplier-two").await;
    f.storage
        .save_supplier_tag("second-tag", second, second)
        .await
        .unwrap();
    f.storage
        .edit_supplier_tags(&["supplier-two".into()], &["second-tag".into()], false)
        .await
        .unwrap();
    let mut group = f
        .storage
        .authenticate_api_key(&f.token)
        .await
        .unwrap()
        .unwrap()
        .group;
    group.tag_ids.push("second-tag".into());
    f.storage
        .save_api_group(&group, Some(group.revision))
        .await
        .unwrap();
    let mut settings = f.storage.gateway_settings().await.unwrap();
    settings.scheduler_mode = "traditional".into();
    f.storage.save_gateway_settings(&settings).await.unwrap();
    f.storage
        .save_supplier_scheduling(
            "supplier-two",
            &codex2api_storage::SupplierScheduling {
                priority: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    f
}

#[tokio::test]
async fn cross_channel_failover_records_each_actual_wire_path() {
    let f = failover_fixture("antigravity", "chatgpt").await;
    *f.mock.antigravity_reply.lock().unwrap() = Some((429, json!({"error":{
        "code":429,"status":"RESOURCE_EXHAUSTED","message":"You have exhausted your capacity on this model.",
        "details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"QUOTA_EXHAUSTED"},
            {"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"120s"}]
    }}).to_string()));
    let response = f.router.clone().oneshot(request("/v1/messages", &f.token,
        json!({"model":"custom-model","max_tokens":100,"messages":[{"role":"user","content":"hi"}]}))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["type"], "message");
    let calls: Vec<String> = f
        .mock
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|call| call.0.clone())
        .collect();
    assert_eq!(
        calls,
        [
            "/v1internal:streamGenerateContent",
            "/backend-api/codex/responses"
        ]
    );
    let attempt: (String, Option<String>, String) =
        sqlx::query_as("SELECT account_provider_id,upstream_endpoint,recovery FROM usage_attempts")
            .fetch_one(f.storage.pool())
            .await
            .unwrap();
    assert_eq!(
        attempt,
        (
            "antigravity".into(),
            Some(calls[0].clone()),
            "failover".into()
        )
    );
    let final_record: (String, Option<String>, String) =
        sqlx::query_as("SELECT account_provider_id,upstream_endpoint,status FROM usage_records")
            .fetch_one(f.storage.pool())
            .await
            .unwrap();
    assert_eq!(
        final_record,
        ("chatgpt".into(), Some(calls[1].clone()), "completed".into())
    );
}

#[tokio::test]
async fn conversion_failure_after_failover_does_not_inherit_the_previous_destination() {
    let f = failover_fixture("chatgpt", "antigravity").await;
    *f.mock.status.lock().unwrap() = Some(429);
    // Codex accepts a remote image, but Gemini conversion rejects it before send.
    let response = f
        .router
        .clone()
        .oneshot(request(
            "/v1/responses",
            &f.token,
            json!({"model":"custom-model","input":[{"role":"user","content":[
                {"type":"input_image","image_url":"https://example.test/image.png"}
            ]}]}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    json_body(response).await;
    let calls: Vec<_> = f
        .mock
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|call| call.0.clone())
        .collect();
    assert_eq!(calls, ["/backend-api/codex/responses"]);
    let attempt: (String, Option<String>) = sqlx::query_as(
        "SELECT account_provider_id,upstream_endpoint FROM usage_attempts WHERE recovery='failover'",
    ).fetch_one(f.storage.pool()).await.unwrap();
    assert_eq!(attempt, ("chatgpt".into(), Some(calls[0].clone())));
    let record: (String, Option<String>, String) =
        sqlx::query_as("SELECT account_provider_id,upstream_endpoint,status FROM usage_records")
            .fetch_one(f.storage.pool())
            .await
            .unwrap();
    assert_eq!(record, ("antigravity".into(), None, "failed".into()));
}

//! Full executor/ledger regressions for the API-key wires. Only upstream HTTP is mocked.
use super::*;
use std::collections::VecDeque;

struct WireMock {
    replies: Mutex<VecDeque<(u16, String)>>,
    calls: std::sync::atomic::AtomicUsize,
    change_group: Option<(Storage, ApiGroup)>,
}
impl codex2api_upstream::HttpTransport for WireMock {
    fn send(&self, request: reqwest::RequestBuilder) -> codex2api_upstream::HttpFuture {
        let request = request.build();
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
            None,
            codex2api_suppliers::api::ApiCredentials {
                api_key: "fixture-key".into(),
                base_url: "https://example.test/v1".into(),
                wire: wire.into(),
                auth_scheme: "bearer".into(),
                retry_statuses: vec![503],
            },
            None,
        )
        .await
        .unwrap();
    f.storage
        .add_supplier_model(&account.id, "custom-model")
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

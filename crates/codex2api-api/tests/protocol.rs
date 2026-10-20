//! Full router, policy, scheduler, provider transport and wallet tests with only network I/O mocked.
use axum::{
    body::{Body, Bytes, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::{
    ApiGroup, ModelConfig, ModelPrice, NewSupplierAccount, Storage, User, UserKind,
    WalletAdjustment,
};
use futures::StreamExt;
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tower::ServiceExt;

#[path = "grok/scheduling.rs"]
mod grok_scheduling;
#[path = "execution/wires.rs"]
mod wire_execution;

#[path = "messages/mod.rs"]
mod messages;

#[derive(Default)]
struct Gate {
    hold: AtomicBool,
    released: AtomicBool,
    notify: tokio::sync::Notify,
}
impl Gate {
    fn release(&self) {
        self.released.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
    async fn wait(&self) {
        let wait = self.notify.notified();
        tokio::pin!(wait);
        wait.as_mut().enable();
        if self.hold.load(Ordering::Acquire) && !self.released.load(Ordering::Acquire) {
            wait.await;
        }
    }
}
#[derive(Default)]
struct Mock {
    requests: Mutex<Vec<(String, http::HeaderMap, Value)>>,
    status: Mutex<Option<u16>>,
    ai_status: Mutex<Option<u16>>,
    gate: Arc<Gate>,
    truncate: AtomicBool,
    failed_usage: AtomicBool,
    partial_output: AtomicBool,
    response_content_type: Mutex<Option<Option<String>>>,
    grok_rejections: Mutex<std::collections::HashMap<String, (u16, String)>>,
    antigravity_reply: Mutex<Option<(u16, String)>>,
}
fn response_value() -> Value {
    json!({"object":"response","id":"resp_fixture","created_at":100,"model":"custom-model","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello"}]}],"usage":{"input_tokens":10000,"output_tokens":5000,"input_tokens_details":{"cached_tokens":2000},"output_tokens_details":{"reasoning_tokens":1000}},"supplier_account_id":"private-supplier"})
}
impl codex2api_upstream::HttpTransport for Mock {
    fn send(&self, request: reqwest::RequestBuilder) -> codex2api_upstream::HttpFuture {
        let built = request.build();
        let gate = self.gate.clone();
        let status = *self.status.lock().unwrap();
        let ai_status = *self.ai_status.lock().unwrap();
        let truncate = self.truncate.load(Ordering::Relaxed);
        let failed_usage = self.failed_usage.load(Ordering::Relaxed);
        let partial_output = self.partial_output.load(Ordering::Relaxed);
        let response_content_type = self.response_content_type.lock().unwrap().clone();
        let antigravity_reply = self.antigravity_reply.lock().unwrap().clone();
        let grok_rejection = built
            .as_ref()
            .ok()
            .and_then(|request| request.headers().get("x-userid"))
            .and_then(|id| id.to_str().ok())
            .and_then(|id| self.grok_rejections.lock().unwrap().get(id).cloned());
        let decoded = built.as_ref().ok().and_then(|request| {
            request
                .body()
                .and_then(|body| body.as_bytes())
                .and_then(|bytes| codex2api_upstream::decode_body(bytes, request.headers()).ok())
        });
        if let (Ok(request), Some(body)) = (&built, &decoded) {
            self.requests.lock().unwrap().push((
                request.url().path().into(),
                request.headers().clone(),
                body.clone(),
            ));
        }
        Box::pin(async move {
            let request = built?;
            if let Some((status, body)) = grok_rejection {
                return Ok(http::Response::builder()
                    .status(status)
                    .header("retry-after", "120")
                    .header("x-should-retry", "false")
                    .header(
                        "content-type",
                        if body.starts_with("data:") {
                            "text/event-stream"
                        } else {
                            "application/json"
                        },
                    )
                    .body(reqwest::Body::from(body))
                    .unwrap()
                    .into());
            }
            assert_eq!(
                request.headers()["authorization"],
                "Bearer fixture-supplier"
            );
            assert_eq!(request.url().scheme(), "https");
            if request.headers().contains_key("x-xai-token-auth") {
                assert_eq!(request.url().host_str(), Some("cli-chat-proxy.grok.com"));
                assert!(request.url().path().starts_with("/v1/"));
                assert_eq!(request.headers()["x-xai-token-auth"], "xai-grok-cli");
            } else if !request.url().path().starts_with("/v1internal:") {
                // Antigravity requests are checked against Cloud Code Assist below.
                let host = if request.url().path().ends_with("/responses/input_tokens") {
                    "api.openai.com"
                } else {
                    "chatgpt.com"
                };
                assert_eq!(request.url().host_str(), Some(host));
            }
            if request.method() == http::Method::GET {
                let id = request.headers()["chatgpt-account-id"].to_str().unwrap();
                return Ok(http::Response::builder().header("content-type","application/json").body(reqwest::Body::from(json!({"accounts":[{"id":id,"workspace_backend_origin":"NO_CONSTRAINT","account_routing_override":"NO_CONSTRAINT"}]}).to_string())).unwrap().into());
            }
            if request.url().path().starts_with("/v1internal:") {
                if let Some((status, body)) = antigravity_reply {
                    return Ok(http::Response::builder()
                        .status(status)
                        .header(
                            "content-type",
                            if body.starts_with("data:") {
                                "text/event-stream"
                            } else {
                                "application/json"
                            },
                        )
                        .header("retry-after", "120")
                        .header("x-should-retry", "true")
                        .body(reqwest::Body::from(body))
                        .unwrap()
                        .into());
                }
                assert!(
                    request
                        .url()
                        .host_str()
                        .unwrap()
                        .ends_with("cloudcode-pa.googleapis.com")
                );
                assert_eq!(request.url().query(), Some("alt=sse"));
                let envelope = decoded.unwrap();
                assert_eq!(envelope["userAgent"], "antigravity");
                if let Some(status) = status {
                    let (rpc, message, details) = if status == 429 {
                        (
                            "RESOURCE_EXHAUSTED",
                            "You have exhausted your capacity on this model. private-supplier",
                            json!([{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"1799s"},
                                {"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"QUOTA_EXHAUSTED","domain":"cloudcode-pa.googleapis.com"}]),
                        )
                    } else {
                        ("UNAVAILABLE", "The model is overloaded.", json!([]))
                    };
                    return Ok(http::Response::builder()
                        .status(status)
                        .header("content-type", "application/json")
                        .body(reqwest::Body::from(
                            json!({"error":{"code":status,"message":message,"status":rpc,"details":details}})
                                .to_string(),
                        ))
                        .unwrap()
                        .into());
                }
                let chunks = [
                    json!({"response":{"candidates":[{"content":{"role":"model","parts":[{"text":"Hello"}]},"index":0}],"responseId":"gemini-fixture"}}),
                    json!({"response":{"candidates":[{"content":{"role":"model","parts":[]},"index":0,"finishReason":"STOP"}],
                        "usageMetadata":{"promptTokenCount":10000,"cachedContentTokenCount":2000,"candidatesTokenCount":4000,"thoughtsTokenCount":1000,"totalTokenCount":15000}}}),
                ];
                let body = chunks
                    .iter()
                    .map(|chunk| format!("data: {chunk}\n\n"))
                    .collect::<String>();
                return Ok(http::Response::builder()
                    .header("content-type", "text/event-stream")
                    .header("set-cookie", "private-supplier")
                    .body(reqwest::Body::from(body))
                    .unwrap()
                    .into());
            }
            if let Some(status) = status {
                let code = match status {
                    402 => "payment_required",
                    429 => "rate_limit_exceeded",
                    _ => "server_error",
                };
                let mut value = json!({"error":{"code":code,"message":"private-supplier","retry_after_seconds":7}});
                if let Some(ai_status) = ai_status {
                    value["error"]["status_code"] = ai_status.into();
                }
                if failed_usage {
                    value["usage"] = response_value()["usage"].clone();
                    value["status"] = "completed".into();
                }
                if partial_output {
                    value["output"] = json!([{"type":"message","content":[{"type":"output_text","text":"partial"}]}]);
                }
                if status == 402 {
                    value = json!({"response":value});
                }
                return Ok(http::Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .header("retry-after", "7")
                    .header("set-cookie", "private-supplier")
                    .body(reqwest::Body::from(value.to_string()))
                    .unwrap()
                    .into());
            }
            if request.url().path().ends_with("/alpha/search") {
                return Ok(http::Response::builder().header("content-type", "application/json")
                    .body(reqwest::Body::from(json!({"output":"Found a real fixture result", "encrypted_output":"search-context", "results":[{"title":"Fixture", "url":"https://example.test/", "metadata":{"account":"public result"}}]}).to_string())).unwrap().into());
            }
            if request.url().path().ends_with("/responses/input_tokens") {
                return Ok(http::Response::builder()
                    .header("content-type", "application/json")
                    .body(reqwest::Body::from(
                        json!({"object":"response.input_tokens","input_tokens":123}).to_string(),
                    ))
                    .unwrap()
                    .into());
            }
            if request.url().path().ends_with("/responses/compact") {
                return Ok(http::Response::builder().header("content-type", "application/json")
                    .body(reqwest::Body::from(json!({"id":"cmp_fixture","object":"response.compaction","output":[{"type":"compaction","encrypted_content":"compacted-context"}],"usage":{"input_tokens":10000,"output_tokens":5000,"input_tokens_details":{"cached_tokens":2000}}}).to_string())).unwrap().into());
            }

            if request.url().path().contains("/images/") {
                return Ok(http::Response::builder().header("content-type","application/json").body(reqwest::Body::from(json!({"created":100,"data":[{"url":"https://example.test/one.png","width":1024,"height":1024},{"url":"https://example.test/two.png","width":1024,"height":1024}],"usage":{"input_tokens":123,"output_tokens":456,"input_tokens_details":{"cached_tokens":23}}}).to_string())).unwrap().into());
            }
            if failed_usage {
                let mut value = response_value();
                value["status"] = "failed".into();
                value["output"] = json!([]);
                value["error"] = json!({"code":"usage_limit_reached","resets_at":2000000000,"message":"private-supplier"});
                if let Some(ai_status) = ai_status {
                    value["error"]["status_code"] = ai_status.into();
                }
                let body = format!(
                    "data: {}\n\n",
                    json!({"type":"response.failed","response":value})
                );
                return Ok(http::Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(reqwest::Body::from(body))
                    .unwrap()
                    .into());
            }
            let first = format!(
                "data: {}\n\ndata: {}\n\n",
                json!({"type":"response.created","response":{"id":"resp_fixture","model":"custom-model","created_at":100,"status":"in_progress"}}),
                json!({"type":"response.output_text.delta","delta":"Hello"})
            );
            let last = format!(
                "data: {}\n\n",
                json!({"type":"response.completed","response":response_value()})
            );
            let bytes =
                futures::stream::once(async move { Ok::<_, std::io::Error>(Bytes::from(first)) })
                    .chain(futures::stream::once(async move {
                        gate.wait().await;
                        Ok(Bytes::from(if truncate { String::new() } else { last }))
                    }));
            let mut response = http::Response::builder().header("set-cookie", "private-supplier");
            if let Some(content_type) =
                response_content_type.unwrap_or_else(|| Some("text/event-stream".into()))
            {
                response = response.header("content-type", content_type);
            }
            Ok(response
                .body(reqwest::Body::wrap_stream(bytes))
                .unwrap()
                .into())
        })
    }
}
struct Fixture {
    _directory: codex2api_storage::test_support::TestDirectory,
    storage: Storage,
    state: codex2api_api::ApiState,
    router: axum::Router,
    token: String,
    key_id: String,
    mock: Arc<Mock>,
}
async fn fixture(provider: &str) -> Fixture {
    let directory = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(directory.path().join("gateway-pg"))
        .await
        .unwrap();
    let user = User {
        id: "user".into(),
        username: "fixture".into(),
        password_hash: "fixture".into(),
        name: "Fixture".into(),
        email: "fixture@example.test".into(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
        kind: UserKind::Regular,
    };
    storage.save_user(&user, None).await.unwrap();
    storage
        .adjust_user_wallet(
            &user.id,
            storage.require_admin_user().await.unwrap().id,
            WalletAdjustment {
                request_id: "fund".into(),
                amount_cents: 100,
                expected_revision: 1,
                reason: None,
            },
        )
        .await
        .unwrap();
    let model = ModelConfig {
        // Catalog platforms label the model's vendor; Antigravity is a supplier
        // channel, so the model it serves keeps a vendor label.
        platform: if provider == codex2api_common::ANTIGRAVITY {
            codex2api_common::GEMINI.into()
        } else {
            provider.into()
        },
        model: "custom-model".into(),
        kind: "text".into(),
        enabled: true,
        deleted: false,
        revision: 0,
    };
    storage
        .save_model_config(
            &model,
            &[ModelPrice {
                model: model.model.clone(),
                tier: "standard".into(),
                min_input_tokens: 0,
                max_input_tokens: None,
                input_rate: 1_000_000,
                cached_rate: 100_000,
                cache_write_rate: 1_000_000,
                output_rate: 2_000_000,
                source: "custom".into(),
                revision: 0,
            }],
            &[],
            None,
        )
        .await
        .unwrap();
    add_supplier(&storage, provider, "supplier").await;
    storage
        .save_supplier_tag("tag", provider, "Pool")
        .await
        .unwrap();
    storage
        .edit_supplier_tags(&["supplier".into()], &["tag".into()], false)
        .await
        .unwrap();
    storage
        .save_api_group(
            &ApiGroup {
                id: "group".into(),
                name: "Group".into(),
                description: "Description".into(),
                enabled: true,
                tag_ids: vec!["tag".into()],
                models: vec![codex2api_common::ModelRule::granted("custom-model")],
                multiplier: "1.5".into(),
                search_price_usd: (provider == "chatgpt").then(|| "0.02".into()),
                concurrency_limit: 1,
                rpm_limit: 0,
                revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    let key = storage
        .user_store()
        .create_api_key("user", "Key", "group")
        .await
        .unwrap();
    let mock = Arc::new(Mock::default());
    let accounts = codex2api_suppliers::SupplierAccountStore::open(storage.clone());
    let auth = codex2api_suppliers::AuthService::new(accounts).unwrap();
    let pool = codex2api_upstream::UpstreamPool::new(auth).with_transport(mock.clone());
    let state = codex2api_api::ApiState::new(storage.clone(), pool);
    let router = codex2api_api::router(state.clone());
    Fixture {
        _directory: directory,
        storage,
        state,
        router,
        token: key.secret,
        key_id: key.key.id,
        mock,
    }
}
async fn add_supplier(storage: &Storage, provider: &str, id: &str) {
    let (mut supplier, tokens) = if provider == "grok" {
        let identity = codex2api_suppliers::providers::grok::GrokIdentity::generate();
        (
            NewSupplierAccount::pending_identity(
                &identity.installation_id,
                &identity.os_type,
                &identity.os_version,
                &identity.arch,
                identity.fingerprint_json().unwrap(),
            ),
            codex2api_suppliers::grok::GrokCredentials {
                access_token: "fixture-supplier".into(),
                refresh_token: None,
                id_token: None,
                expires_at: Some(chrono::Utc::now().timestamp() + 3600),
                profile: json!({"userId":id}),
            }
            .to_supplier_tokens(id),
        )
    } else if provider == "antigravity" {
        let identity = codex2api_suppliers::antigravity::AntigravityIdentity::generate();
        (
            NewSupplierAccount::pending_identity(
                &identity.installation_id,
                &identity.os_type,
                &identity.os_version,
                &identity.arch,
                identity.fingerprint_json().unwrap(),
            ),
            codex2api_suppliers::antigravity::AntigravityCredentials {
                access_token: "fixture-supplier".into(),
                refresh_token: None,
                expires_at: Some(chrono::Utc::now().timestamp() + 3600),
                project_id: Some("fixture-project".into()),
                tier: None,
                profile: json!({"email":"fixture@example.test"}),
            }
            .to_supplier_tokens(id),
        )
    } else {
        let identity = codex2api_suppliers::AccountIdentity::new(
            id,
            uuid::Uuid::new_v4().to_string(),
            codex2api_suppliers::HostRuntime {
                terminal: "fixture".into(),
                os_type: "Linux".into(),
                os_version: "6".into(),
                arch: "x86_64".into(),
            },
        );
        (
            NewSupplierAccount::pending_identity(
                &identity.installation_id,
                &identity.os_type,
                &identity.os_version,
                &identity.arch,
                identity.fingerprint_json().unwrap(),
            ),
            codex2api_suppliers::AuthDotJson::chatgpt(
                codex2api_suppliers::TokenData {
                    access_token: "fixture-supplier".into(),
                    account_id: Some("workspace".into()),
                    ..Default::default()
                },
                Some(chrono::Utc::now()),
            )
            .to_supplier_tokens(id),
        )
    };
    supplier.id = Some(id.into());
    supplier.provider_id = provider.into();
    supplier.chatgpt_account_id = Some("workspace".into());
    supplier.chatgpt_user_id = Some(id.into());
    storage
        .save_authorized_account(supplier, tokens, None)
        .await
        .unwrap();
    storage
        .add_supplier_model(id, "custom-model")
        .await
        .unwrap();
}
fn request(path: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .header("cookie", "consumer-browser-session")
        .header("user-agent", "consumer-application")
        .header("originator", "consumer-client")
        .header("openai-project", "consumer-project")
        .header("chatgpt-account-id", "consumer-account")
        .header("x-codex-installation-id", "consumer-installation")
        .header("x-codex-guardian", "reviewer")
        .header("x-api-group", "group")
        .body(Body::from(body.to_string()))
        .unwrap()
}
async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

#[tokio::test]
async fn early_rejections_keep_key_ownership_and_do_not_charge_or_duplicate_usage() {
    let f = fixture("chatgpt").await;
    let oversized = Bytes::from(vec![b' '; codex2api_upstream::MAX_REQUEST_BYTES + 1]);
    for (body, expected) in [
        (oversized, StatusCode::PAYLOAD_TOO_LARGE),
        (Bytes::from_static(b"not-json"), StatusCode::BAD_REQUEST),
    ] {
        let response = f
            .router
            .clone()
            .oneshot(
                Request::post("/v1/responses/compact")
                    .header("authorization", format!("Bearer {}", f.token))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        to_bytes(response.into_body(), 8192).await.unwrap();
    }
    let response = f
        .router
        .clone()
        .oneshot(request(
            "/v1/responses/compact",
            &f.token,
            json!({"model":"unavailable-model","input":[]}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    json_body(response).await;
    let page = f.storage.query_usage(&Default::default()).await.unwrap();
    assert_eq!(page.total, 3);
    for row in &page.records {
        assert_eq!(row.user_id.as_deref(), Some("user"));
        assert_eq!(row.key_id.as_deref(), Some(f.key_id.as_str()));
        assert_eq!(row.group_id.as_deref(), Some("group"));
        assert_eq!(row.status, "failed");
        assert_eq!(row.billing_status, "not_charged");
        assert_eq!(row.cost_nano_usd, Some(0));
        assert_eq!(row.wallet_debit_cents, 0);
        assert!(row.input_tokens.is_none() && row.output_tokens.is_none());
        assert!(row.error_message.is_some());
    }
    assert!(
        page.records
            .iter()
            .any(|row| row.model.as_deref() == Some("unavailable-model"))
    );
    assert!(f.mock.requests.lock().unwrap().is_empty());
    assert_eq!(
        f.storage.user("user").await.unwrap().unwrap().wallet_cents,
        100
    );
    let user = f
        .storage
        .user_store()
        .usage("user", &Default::default())
        .await
        .unwrap();
    assert_eq!(user["total"], 3);
    assert!(!user.to_string().contains("error_message"));
    assert!(!user.to_string().contains("upstream_request_id"));

    f.storage
        .set_account_status("supplier", codex2api_storage::SupplierStatus::Disabled)
        .await
        .unwrap();
    let response = f
        .router
        .clone()
        .oneshot(request(
            "/v1/responses",
            &f.token,
            json!({"model":"custom-model","input":"Hello"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    json_body(response).await;
    let page = f.storage.query_usage(&Default::default()).await.unwrap();
    assert_eq!(page.total, 4);
    let row = &page.records[0];
    assert_eq!(row.http_status, Some(503));
    assert_eq!(row.billing_status, "not_charged");
    assert_eq!(row.cost_nano_usd, Some(0));
    assert_eq!(row.user_agent.as_deref(), Some("consumer-application"));
    f.storage
        .set_account_status("supplier", codex2api_storage::SupplierStatus::Active)
        .await
        .unwrap();

    *f.mock.status.lock().unwrap() = Some(500);
    let response = f
        .router
        .clone()
        .oneshot(request(
            "/v1/responses",
            &f.token,
            json!({"model":"custom-model","input":"Hello"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    json_body(response).await;
    assert_eq!(
        f.storage
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        5
    );
    f.storage.close().await;
}

#[tokio::test]
async fn both_platforms_and_protocols_settle_actual_usage_before_completion() {
    for provider in ["chatgpt", "grok"] {
        for chat in [false, true] {
            for stream in [false, true] {
                let fixture = fixture(provider).await;
                let path = if chat {
                    "/v1/chat/completions"
                } else {
                    "/v1/responses"
                };
                let mut body = if chat {
                    json!({"model":"custom-model","messages":[{"role":"user","content":"Hello"}]})
                } else {
                    json!({"model":"custom-model","input":"Hello"})
                };
                body["stream"] = stream.into();
                body["store"] = true.into();
                body["client_metadata"] = json!({"session_id":"consumer-session","x-codex-installation-id":"consumer-installation","supplier":"private-caller"});
                body["metadata"] = json!({"local":"caller-metadata"});
                body["user"] = "private-caller".into();
                body["temperature"] = json!(0.4);
                body[if chat {
                    "max_completion_tokens"
                } else {
                    "max_output_tokens"
                }] = json!(123);
                if chat && stream {
                    body["stream_options"] = json!({"include_usage":true});
                }
                let response = fixture
                    .router
                    .clone()
                    .oneshot(request(path, &fixture.token, body))
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::OK,
                    "{provider} {path} stream={stream}"
                );
                assert!(response.headers().get("set-cookie").is_none());
                let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
                let text = String::from_utf8(bytes.to_vec()).unwrap();
                assert!(text.contains("Hello"));
                assert!(!text.contains("private-supplier"));
                if stream && chat {
                    assert!(text.ends_with("data: [DONE]\n\n"));
                    assert!(text.contains("\"choices\":[]"));
                    assert!(text.contains("\"completion_tokens\":5000"));
                }
                if !stream {
                    let result: Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(
                        result["object"],
                        if chat { "chat.completion" } else { "response" }
                    );
                }
                assert_eq!(
                    fixture
                        .storage
                        .user("user")
                        .await
                        .unwrap()
                        .unwrap()
                        .wallet_cents,
                    98
                );
                let record:(String,Option<i64>,Option<i64>,Option<i64>,Option<i64>)=sqlx::query_as("SELECT status,input_tokens,output_tokens,reasoning_tokens,cost_nano_usd FROM usage_records").fetch_one(fixture.storage.pool()).await.unwrap();
                let ttft: Option<i64> =
                    sqlx::query_scalar("SELECT first_token_ms FROM usage_records")
                        .fetch_one(fixture.storage.pool())
                        .await
                        .unwrap();
                // Both supplier clients always stream upstream, so TTFT is observable.
                assert!(ttft.is_some(), "{provider} stream={stream}");
                assert_eq!(
                    record,
                    (
                        "completed".into(),
                        Some(10000),
                        Some(5000),
                        Some(1000),
                        Some(27_300_000)
                    )
                );
                assert_eq!(
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM usage_records")
                        .fetch_one(fixture.storage.pool())
                        .await
                        .unwrap(),
                    1
                );
                let calls = fixture.mock.requests.lock().unwrap();
                let (path, headers, body) = &calls[0];
                assert_ne!(
                    headers["authorization"].to_str().unwrap(),
                    format!("Bearer {}", fixture.token)
                );
                for name in [
                    "cookie",
                    "x-api-group",
                    "openai-project",
                    "x-codex-guardian",
                ] {
                    assert!(!headers.contains_key(name), "{name}");
                }
                assert_ne!(headers["user-agent"], "consumer-application");
                assert!(body.get("metadata").is_none());
                assert!(body.get("user").is_none());
                assert!(body.get("max_completion_tokens").is_none());
                if provider == "chatgpt" {
                    assert_eq!(path, "/backend-api/codex/responses");
                    assert_eq!(body["store"], false);
                    assert_eq!(body["stream"], true);
                    assert_eq!(headers["originator"], "codex_cli_rs");
                    assert_eq!(headers["chatgpt-account-id"], "workspace");
                    assert_eq!(headers["session-id"], "consumer-session");
                    assert!(!headers.contains_key("thread-id"));
                    assert_eq!(body["client_metadata"]["session_id"], "consumer-session");
                    assert!(body["client_metadata"].get("thread_id").is_none());
                    assert!(body["client_metadata"].get("supplier").is_none());
                    assert!(body.get("temperature").is_none());
                    assert!(body.get("max_output_tokens").is_none());
                    assert!(
                        uuid::Uuid::parse_str(
                            body["client_metadata"]["x-codex-installation-id"]
                                .as_str()
                                .unwrap()
                        )
                        .is_ok()
                    );
                    assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
                } else {
                    assert!(body.get("client_metadata").is_none());
                    assert_eq!(body["temperature"], 0.4);
                    assert_eq!(path, "/v1/responses");
                    assert_eq!(body["max_output_tokens"], 123);
                    assert_eq!(body["stream"], true);
                    assert_eq!(body["store"], false);
                    assert!(!headers.contains_key("chatgpt-account-id"));
                    assert!(!headers.contains_key("x-codex-installation-id"));
                    assert!(!headers.contains_key("originator"));
                    assert!(headers.contains_key("x-xai-token-auth"));
                    assert_eq!(headers["x-grok-client-identifier"], "grok-pager");
                    assert!(
                        headers["user-agent"]
                            .to_str()
                            .unwrap()
                            .contains("grok-pager/")
                    );
                    assert!(
                        uuid::Uuid::parse_str(headers["x-grok-req-id"].to_str().unwrap()).is_ok()
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn disconnection_keeps_generation_lease_and_settles_without_oauth() {
    let fixture = fixture("chatgpt").await;
    fixture.mock.gate.hold.store(true, Ordering::Release);
    let second = fixture
        .storage
        .user_store()
        .create_api_key("user", "Second", "group")
        .await
        .unwrap();
    let body = json!({"model":"custom-model","input":"Hello","stream":true});
    let response = fixture
        .router
        .clone()
        .oneshot(request("/v1/responses", &fixture.token, body.clone()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(fixture.state.runtime().current_for_key(&fixture.key_id), 1);
    let rejected = fixture
        .router
        .clone()
        .oneshot(request("/v1/responses", &second.secret, body))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        json_body(rejected).await["error"]["code"],
        "api_group_concurrency_exceeded"
    );
    drop(response);
    assert_eq!(fixture.state.runtime().current_for_key(&fixture.key_id), 1);
    fixture.mock.gate.release();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while fixture.state.runtime().current_for_key(&fixture.key_id) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        fixture
            .storage
            .user("user")
            .await
            .unwrap()
            .unwrap()
            .wallet_cents,
        98
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM usage_records WHERE status='completed'")
            .fetch_one(fixture.storage.pool())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn supplier_throttle_is_forwarded_without_failover_or_persistent_ban() {
    for provider in ["chatgpt", "grok"] {
        let fixture = fixture(provider).await;
        *fixture.mock.status.lock().unwrap() = Some(429);
        let response = fixture
            .router
            .clone()
            .oneshot(request(
                "/v1/responses",
                &fixture.token,
                json!({"model":"custom-model","input":"Hello"}),
            ))
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let value = json_body(response).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{provider}: {value}");
        assert_eq!(headers["retry-after"], "7");
        assert_eq!(value["error"]["code"], "rate_limit_exceeded");
        assert!(!value.to_string().contains("private-supplier"));
        assert_eq!(fixture.mock.requests.lock().unwrap().len(), 1);
        let health = fixture.storage.supplier_health("supplier").await.unwrap();
        assert!(!health.authentication_invalid());
        assert!(!health.payment_required());
        assert!(
            health
                .account_block()
                .and_then(|block| block.until_at)
                .is_none()
        );
        assert_eq!(
            fixture
                .storage
                .user("user")
                .await
                .unwrap()
                .unwrap()
                .wallet_cents,
            100
        );
    }
}

#[tokio::test]
async fn truncated_stream_is_not_reported_as_success_or_free_usage() {
    let fixture = fixture("chatgpt").await;
    fixture.mock.truncate.store(true, Ordering::Release);
    let response=fixture.router.clone().oneshot(request("/v1/chat/completions",&fixture.token,json!({"model":"custom-model","messages":[{"role":"user","content":"Hello"}],"stream":true}))).await.unwrap();
    let text = String::from_utf8(
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(text.contains("upstream_incomplete"));
    assert!(!text.contains("\"finish_reason\":\"stop\""));
    assert!(!text.contains("[DONE]"));
    let (status, tokens, cost): (String, Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT status,input_tokens,cost_nano_usd FROM usage_records")
            .fetch_one(fixture.storage.pool())
            .await
            .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(tokens, None);
    assert_eq!(cost, None);
}

#[tokio::test]
async fn failed_response_with_reported_usage_is_charged_without_replay() {
    for provider in ["chatgpt", "grok"] {
        for stream in [false, true] {
            for ai_status in [Some(429), None] {
                let fixture = fixture(provider).await;
                *fixture.mock.ai_status.lock().unwrap() = ai_status;
                add_supplier(&fixture.storage, provider, "supplier-two").await;
                fixture
                    .storage
                    .edit_supplier_tags(&["supplier-two".into()], &["tag".into()], false)
                    .await
                    .unwrap();
                fixture.mock.failed_usage.store(true, Ordering::Release);
                let (path, body) = if provider == "chatgpt" {
                    (
                        "/v1/chat/completions",
                        json!({"model":"custom-model","messages":[{"role":"user","content":"Hello"}],"stream":stream}),
                    )
                } else {
                    (
                        "/v1/responses",
                        json!({"model":"custom-model","input":"Hello","stream":stream}),
                    )
                };
                let response = fixture
                    .router
                    .clone()
                    .oneshot(request(path, &fixture.token, body))
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    if stream {
                        StatusCode::OK
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                );
                let text = String::from_utf8(
                    to_bytes(response.into_body(), 1024 * 1024)
                        .await
                        .unwrap()
                        .to_vec(),
                )
                .unwrap();
                assert!(text.contains("service_unavailable"));
                assert!(!text.contains("usage_limit_reached"));
                assert!(!text.contains("private-supplier"));
                assert!(!text.contains("[DONE]"));
                assert!(!text.contains("\"finish_reason\":\"stop\""));
                assert_eq!(
                    sqlx::query_scalar::<_, Option<String>>("SELECT error_code FROM usage_records")
                        .fetch_one(fixture.storage.pool())
                        .await
                        .unwrap()
                        .as_deref(),
                    Some("usage_limit_reached")
                );
                let record:(String,Option<i64>,Option<i64>,Option<i64>,Option<i64>)=sqlx::query_as("SELECT status,input_tokens,output_tokens,http_status,cost_nano_usd FROM usage_records").fetch_one(fixture.storage.pool()).await.unwrap();
                assert_eq!(
                    record,
                    (
                        "failed".into(),
                        Some(10000),
                        Some(5000),
                        Some(200),
                        Some(27_300_000)
                    )
                );
                assert_eq!(
                    fixture
                        .storage
                        .user("user")
                        .await
                        .unwrap()
                        .unwrap()
                        .wallet_cents,
                    98
                );
                assert_eq!(fixture.mock.requests.lock().unwrap().len(), 1);
            }
        }
    }
}

#[tokio::test]
async fn physical_http_failure_preserves_reported_usage_and_does_not_replay() {
    for provider in ["chatgpt", "grok"] {
        for status in [500, 429, 402] {
            for chat in [false, true] {
                let fixture = fixture(provider).await;
                add_supplier(&fixture.storage, provider, "supplier-two").await;
                fixture
                    .storage
                    .edit_supplier_tags(&["supplier-two".into()], &["tag".into()], false)
                    .await
                    .unwrap();
                *fixture.mock.status.lock().unwrap() = Some(status);
                fixture.mock.failed_usage.store(true, Ordering::Release);
                let (path, body) = if chat {
                    (
                        "/v1/chat/completions",
                        json!({"model":"custom-model","messages":[{"role":"user","content":"Hello"}]}),
                    )
                } else {
                    (
                        "/v1/responses",
                        json!({"model":"custom-model","input":"Hello"}),
                    )
                };
                let response = fixture
                    .router
                    .clone()
                    .oneshot(request(path, &fixture.token, body))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
                assert!(!response.headers().contains_key("retry-after"));
                let value = json_body(response).await;
                assert_eq!(value["error"]["code"], "no_available_supplier");
                assert!(!value.to_string().contains("private-supplier"));
                let record:(String,Option<i64>,Option<i64>,Option<i64>,Option<i64>)=sqlx::query_as("SELECT status,input_tokens,output_tokens,http_status,cost_nano_usd FROM usage_records").fetch_one(fixture.storage.pool()).await.unwrap();
                assert_eq!(
                    record,
                    (
                        "failed".into(),
                        Some(10000),
                        Some(5000),
                        Some(i64::from(status)),
                        Some(27_300_000)
                    ),
                    "{provider} {status} {path}"
                );
                assert_eq!(
                    fixture
                        .storage
                        .user("user")
                        .await
                        .unwrap()
                        .unwrap()
                        .wallet_cents,
                    98
                );
                assert_eq!(fixture.mock.requests.lock().unwrap().len(), 1);
            }
        }
    }
}

#[tokio::test]
async fn physical_rejection_with_partial_output_preserves_unknown_cost() {
    let fixture = fixture("chatgpt").await;
    *fixture.mock.status.lock().unwrap() = Some(429);
    fixture.mock.partial_output.store(true, Ordering::Release);
    let response = fixture
        .router
        .clone()
        .oneshot(request(
            "/v1/responses",
            &fixture.token,
            json!({"model":"custom-model","input":"Hello"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let (status, cost, billing): (String, Option<i64>, String) =
        sqlx::query_as("SELECT status,cost_nano_usd,billing_status FROM usage_records")
            .fetch_one(fixture.storage.pool())
            .await
            .unwrap();
    assert_eq!(
        (status, cost, billing),
        ("failed".into(), None, "missing_usage".into())
    );
}

#[tokio::test]
async fn images_use_each_platform_transport_and_bill_actual_count() {
    for provider in ["chatgpt", "grok"] {
        for edit in [false, true] {
            let fixture = fixture(provider).await;
            fixture
                .storage
                .save_model_config(
                    &ModelConfig {
                        platform: provider.into(),
                        model: "custom-image".into(),
                        kind: "image".into(),
                        enabled: true,
                        deleted: false,
                        revision: 0,
                    },
                    &[],
                    &[codex2api_storage::ImagePrice {
                        model: "custom-image".into(),
                        resolution: "1K".into(),
                        price_nano_usd: 20_000_000,
                    }],
                    None,
                )
                .await
                .unwrap();
            fixture
                .storage
                .add_supplier_model("supplier", "custom-image")
                .await
                .unwrap();
            // API groups grant only the catalog models they list.
            let group = fixture.storage.api_group("group").await.unwrap().unwrap();
            let mut granted = group.clone();
            granted
                .models
                .push(codex2api_common::ModelRule::granted("custom-image"));
            assert!(
                fixture
                    .storage
                    .save_api_group(&granted, Some(group.revision))
                    .await
                    .unwrap()
            );
            let path = if edit {
                "/v1/images/edits"
            } else {
                "/v1/images/generations"
            };
            let mut body =
                json!({"model":"custom-image","prompt":"A tree","n":3,"size":"1024x1024"});
            if edit {
                body["image"] = json!({"url":"https://example.test/input.png"});
            }
            let response = fixture
                .router
                .clone()
                .oneshot(request(path, &fixture.token, body))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{provider} {path}");
            let result = json_body(response).await;
            assert_eq!(result["data"].as_array().unwrap().len(), 2);
            let (count, cost): (Option<i64>, Option<i64>) =
                sqlx::query_as("SELECT image_count,cost_nano_usd FROM usage_records")
                    .fetch_one(fixture.storage.pool())
                    .await
                    .unwrap();
            assert_eq!((count, cost), (Some(2), Some(60_000_000)));
            let user_agent: Option<String> =
                sqlx::query_scalar("SELECT user_agent FROM usage_records")
                    .fetch_one(fixture.storage.pool())
                    .await
                    .unwrap();
            assert_eq!(user_agent.as_deref(), Some("consumer-application"));
            let tokens: (Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as(
                "SELECT input_tokens,output_tokens,cached_tokens FROM usage_records",
            )
            .fetch_one(fixture.storage.pool())
            .await
            .unwrap();
            assert_eq!(tokens, (Some(123), Some(456), Some(23)));
            assert_eq!(
                fixture
                    .storage
                    .user("user")
                    .await
                    .unwrap()
                    .unwrap()
                    .wallet_cents,
                94
            );
            let requests = fixture.mock.requests.lock().unwrap();
            let (actual, headers, body) = &requests[0];
            assert!(actual.ends_with(if edit {
                "/images/edits"
            } else {
                "/images/generations"
            }));
            assert_eq!(headers["authorization"], "Bearer fixture-supplier");
            for name in [
                "cookie",
                "x-api-group",
                "openai-project",
                "x-codex-guardian",
            ] {
                assert!(!headers.contains_key(name));
            }
            if provider == "chatgpt" {
                assert!(actual.starts_with("/backend-api/codex/"));
                assert_eq!(headers["originator"], "codex_cli_rs");
                assert!(
                    uuid::Uuid::parse_str(headers["x-codex-image-turn-id"].to_str().unwrap())
                        .is_ok()
                );
                if edit {
                    assert_eq!(
                        body["images"][0]["image_url"],
                        "https://example.test/input.png"
                    );
                }
            } else {
                assert!(headers.contains_key("x-xai-token-auth"));
                assert!(!headers.contains_key("originator"));
                assert!(!headers.contains_key("x-codex-image-turn-id"));
                assert_eq!(body["resolution"], "1k");
                if edit {
                    assert_eq!(body["image"]["type"], "image_url");
                }
            }
        }
    }
}

#[tokio::test]
async fn previous_responses_keep_supplier_and_consumer_ownership_for_native_grok() {
    let fixture = fixture("grok").await;
    add_supplier(&fixture.storage, "grok", "supplier-two").await;
    fixture
        .storage
        .edit_supplier_tags(&["supplier-two".into()], &["tag".into()], false)
        .await
        .unwrap();
    let initial = fixture
        .router
        .clone()
        .oneshot(request(
            "/v1/responses",
            &fixture.token,
            json!({"model":"custom-model","input":"Hello"}),
        ))
        .await
        .unwrap();
    assert_eq!(initial.status(), StatusCode::OK);
    assert_eq!(json_body(initial).await["id"], "resp_fixture");
    let selected = fixture.mock.requests.lock().unwrap()[0].1["x-userid"]
        .to_str()
        .unwrap()
        .to_owned();
    let follow =
        json!({"model":"custom-model","input":"Next","previous_response_id":"resp_fixture"});
    let second = fixture
        .router
        .clone()
        .oneshot(request("/v1/responses", &fixture.token, follow.clone()))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let _ = json_body(second).await;
    assert_eq!(
        fixture.mock.requests.lock().unwrap()[1].1["x-userid"],
        selected
    );
    let other = User {
        id: "other-user".into(),
        username: "other".into(),
        password_hash: "fixture".into(),
        name: "Other".into(),
        email: "other@example.test".into(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
        kind: UserKind::Regular,
    };
    fixture.storage.save_user(&other, None).await.unwrap();
    fixture
        .storage
        .adjust_user_wallet(
            &other.id,
            fixture.storage.require_admin_user().await.unwrap().id,
            WalletAdjustment {
                request_id: "fund-other".into(),
                amount_cents: 100,
                expected_revision: 1,
                reason: None,
            },
        )
        .await
        .unwrap();
    let other_key = fixture
        .storage
        .user_store()
        .create_api_key(&other.id, "Other", "group")
        .await
        .unwrap();
    let denied = fixture
        .router
        .clone()
        .oneshot(request("/v1/responses", &other_key.secret, follow.clone()))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(denied).await["error"]["code"],
        "previous_response_not_found"
    );
    fixture
        .storage
        .set_account_status(&selected, codex2api_storage::SupplierStatus::Disabled)
        .await
        .unwrap();
    let unavailable = fixture
        .router
        .clone()
        .oneshot(request("/v1/responses", &fixture.token, follow))
        .await
        .unwrap();
    assert_eq!(unavailable.status(), StatusCode::BAD_REQUEST);
    assert_eq!(fixture.mock.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn upstream_sse_framing_preserves_the_callers_response_mode_and_usage() {
    for media_type in [None, Some("application/json")] {
        for stream in [false, true] {
            let fixture = fixture("chatgpt").await;
            *fixture.mock.response_content_type.lock().unwrap() =
                Some(media_type.map(str::to_owned));
            let response = fixture
                .router
                .clone()
                .oneshot(request(
                    "/v1/responses",
                    &fixture.token,
                    json!({"model":"custom-model","input":"Hello","stream":stream}),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
            if stream {
                let body = std::str::from_utf8(&bytes).unwrap();
                assert!(body.contains("response.completed"));
                assert!(!body.contains("upstream_protocol_error"));
            } else {
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(body["status"], "completed");
                assert_eq!(body["output"][0]["content"][0]["text"], "Hello");
            }
            let row: (Option<bool>, String, Option<i64>) =
                sqlx::query_as("SELECT stream,status,input_tokens FROM usage_records")
                    .fetch_one(fixture.storage.pool())
                    .await
                    .unwrap();
            assert_eq!(row, (Some(stream), "completed".into(), Some(10000)));
            assert_eq!(fixture.mock.requests.lock().unwrap().len(), 1);
        }
    }
}

#[tokio::test]
async fn api_rewrite_public_contracts() {
    let f = fixture("chatgpt").await;
    f.storage
        .save_operation_price(
            &codex2api_storage::OperationPrice {
                provider_id: "chatgpt".into(),
                operation: "search".into(),
                price_nano_usd: Some(5_000_000),
                revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    let response=f.router.clone().oneshot(request("/v1/alpha/search",&f.token,json!({
        "id":"search-caller","model":"custom-model","commands":{"search_query":[{"q":"fixture"}]}
    }))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["output"], "Found a real fixture result");
    assert_eq!(body["results"][0]["metadata"]["account"], "public result");
    for path in ["/v1/responses/compact", "/v1/responses/input_tokens"] {
        let response = f
            .router
            .clone()
            .oneshot(request(
                path,
                &f.token,
                json!({"model":"custom-model","input":"Hello"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let body = json_body(response).await;
        assert!(body.get("error").is_none(), "{path}: {body}");
    }
    let mut call = request(
        "/v1/chat/completions",
        &f.token,
        json!({
            "model":"custom-model","messages":[{"role":"system","content":"Be exact"},{"role":"user","content":"Hello"}],
            "tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object","properties":{"account":{"type":"string"}}},"strict":true}}],
            "tool_choice":{"type":"function","function":{"name":"lookup"}},
            "reasoning_effort":"high","metadata":{"account":"caller-data"},"prompt_cache_key":"caller-cache"
        }),
    );
    call.headers_mut()
        .insert("session-id", "caller-session".parse().unwrap());
    call.headers_mut()
        .insert("thread-id", "caller-thread".parse().unwrap());
    // A loopback reverse proxy forwards the caller; a spoofed first hop is ignored.
    call.extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            40000,
        ))));
    call.headers_mut().insert(
        "x-forwarded-for",
        "203.0.113.66, 198.51.100.23".parse().unwrap(),
    );
    let response = f.router.clone().oneshot(call).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value = json_body(response).await;
    assert_eq!(value["choices"][0]["message"]["content"], "Hello");
    assert_eq!(value["metadata"]["account"], "caller-data");
    let calls = f.mock.requests.lock().unwrap();
    let (_, headers, wire) = calls.last().unwrap();
    assert_eq!(headers["session-id"], "caller-session");
    assert_eq!(headers["thread-id"], "caller-thread");
    assert_eq!(wire["prompt_cache_key"], "caller-cache");
    assert!(wire.get("instructions").is_none());
    assert_eq!(wire["input"][0]["role"], "developer");
    assert_eq!(wire["tools"][0]["name"], "lookup");
    assert_eq!(
        wire["tools"][0]["parameters"]["properties"]["account"]["type"],
        "string"
    );
    assert!(
        wire["include"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "reasoning.encrypted_content")
    );
    drop(calls);
    *f.mock.status.lock().unwrap() = Some(200);
    *f.mock.ai_status.lock().unwrap() = Some(429);
    let response = f
        .router
        .clone()
        .oneshot(request(
            "/v1/responses",
            &f.token,
            json!({"model":"custom-model","input":"Hello","stream":true}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["retry-after"], "7");
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
    let failure = json_body(response).await;
    assert_eq!(failure["error"]["code"], "rate_limit_exceeded");
    let records = f.storage.query_usage(&Default::default()).await.unwrap();
    assert_eq!(records.total, 5);
    assert_eq!(
        records
            .records
            .iter()
            .filter(|r| r.status == "completed")
            .count(),
        4
    );
    let chat = records
        .records
        .iter()
        .find(|r| r.endpoint == "/v1/chat/completions")
        .unwrap();
    assert_eq!(chat.client_ip.as_deref(), Some("198.51.100.23"));
    let search = records
        .records
        .iter()
        .find(|r| r.endpoint == "/v1/alpha/search")
        .unwrap();
    assert_eq!(search.client_ip, None);
    assert_eq!(search.cost_nano_usd, Some(20_000_000));
    let count = records
        .records
        .iter()
        .find(|r| r.endpoint == "/v1/responses/input_tokens")
        .unwrap();
    assert_eq!(count.input_tokens, None);
    assert_eq!(count.cost_nano_usd, Some(0));
    assert_eq!(count.billing_status, "not_charged");
}

#[tokio::test]
async fn mixed_pools_skip_unsupported_protocols_even_with_existing_affinity() {
    let f = fixture("antigravity").await;
    add_supplier(&f.storage, "chatgpt", "chatgpt").await;
    f.storage
        .save_supplier_tag("chatgpt", "chatgpt", "ChatGPT")
        .await
        .unwrap();
    f.storage
        .edit_supplier_tags(&["chatgpt".into()], &["chatgpt".into()], false)
        .await
        .unwrap();
    f.storage
        .save_supplier_scheduling(
            "chatgpt",
            &codex2api_storage::SupplierScheduling {
                priority: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut group = f.storage.api_group("group").await.unwrap().unwrap();
    group.tag_ids.push("chatgpt".into());
    f.storage
        .save_api_group(&group, Some(group.revision))
        .await
        .unwrap();
    let mut settings = f.storage.gateway_settings().await.unwrap();
    settings.scheduler_mode = "traditional".into();
    f.storage.save_gateway_settings(&settings).await.unwrap();
    let make = |path| {
        let mut request = request(
            path,
            &f.token,
            json!({"model":"custom-model","input":"hello"}),
        );
        request
            .headers_mut()
            .insert("session-id", "shared-session".parse().unwrap());
        request
    };
    let generated = f
        .router
        .clone()
        .oneshot(make("/v1/responses"))
        .await
        .unwrap();
    assert_eq!(generated.status(), StatusCode::OK);
    json_body(generated).await;
    let compacted = f
        .router
        .clone()
        .oneshot(make("/v1/responses/compact"))
        .await
        .unwrap();
    let status = compacted.status();
    let body = json_body(compacted).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let paths: Vec<_> = f
        .mock
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|(path, _, _)| path.clone())
        .collect();
    assert_eq!(
        paths,
        [
            "/v1internal:streamGenerateContent",
            "/backend-api/codex/responses/compact"
        ]
    );
    f.storage
        .edit_supplier_tags(&["chatgpt".into()], &["chatgpt".into()], true)
        .await
        .unwrap();
    let rejected = f
        .router
        .clone()
        .oneshot(make("/v1/responses/compact"))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json_body(rejected).await["error"]["code"],
        "service_unavailable"
    );
    assert_eq!(f.mock.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn antigravity_long_throttles_preserve_retry_advice_and_partial_usage_without_eviction() {
    for (sse, streaming) in [(false, false), (true, false), (true, true)] {
        let f = fixture("antigravity").await;
        let error = json!({"code":429,"status":"RESOURCE_EXHAUSTED","message":"Rate limited",
            "details":[{"@type":"type.googleapis.com/google.rpc.ErrorInfo","reason":"RATE_LIMIT_EXCEEDED"},
                {"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"90s"}]});
        let body = if sse {
            format!(
                "data: {}\n\n",
                json!({"response":{"error":error,"usageMetadata":{"promptTokenCount":10000,"candidatesTokenCount":5000}}})
            )
        } else {
            json!({"error":error}).to_string()
        };
        *f.mock.antigravity_reply.lock().unwrap() = Some((if sse { 200 } else { 429 }, body));
        let response = f
            .router
            .clone()
            .oneshot(request(
                "/v1/responses",
                &f.token,
                json!({"model":"custom-model","input":"hello","stream":streaming}),
            ))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if streaming {
                StatusCode::OK
            } else {
                StatusCode::TOO_MANY_REQUESTS
            }
        );
        assert_eq!(response.headers()["retry-after"], "120");
        assert_eq!(response.headers()["x-should-retry"], "true");
        let body = String::from_utf8(
            to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(body.contains("rate_limit_exceeded"), "{body}");
        assert_eq!(f.mock.requests.lock().unwrap().len(), 1);
        assert!(
            f.storage
                .supplier_model_blocks("supplier")
                .await
                .unwrap()
                .is_empty()
        );
        let health = f.storage.supplier_health("supplier").await.unwrap();
        assert!(health.quota_exhausted() == false && !health.authentication_invalid());
        let usage: (String, Option<i64>, Option<i64>) =
            sqlx::query_as("SELECT status,input_tokens,output_tokens FROM usage_records")
                .fetch_one(f.storage.pool())
                .await
                .unwrap();
        assert_eq!(usage.0, "failed");
        assert_eq!(usage.1, sse.then_some(10000));
        assert_eq!(usage.2, sse.then_some(5000));
    }
}

#[tokio::test]
async fn antigravity_serves_every_protocol_as_the_desktop_client_and_cools_down_one_model() {
    for (path, stream) in [
        ("/v1/responses", false),
        ("/v1/responses", true),
        ("/v1/chat/completions", false),
        ("/v1/chat/completions", true),
        ("/v1/messages", false),
        ("/v1/messages", true),
    ] {
        let fixture = fixture("antigravity").await;
        let mut body = match path {
            "/v1/responses" => {
                json!({"model":"custom-model","input":"Hello","max_output_tokens":123,"metadata":{"local":"caller-metadata"}})
            }
            "/v1/chat/completions" => {
                json!({"model":"custom-model","messages":[{"role":"user","content":"Hello"}],"max_completion_tokens":123,"metadata":{"local":"caller-metadata"}})
            }
            _ => {
                json!({"model":"custom-model","messages":[{"role":"user","content":"Hello"}],"max_tokens":123})
            }
        };
        body["stream"] = stream.into();
        body["temperature"] = json!(0.4);
        let response = fixture
            .router
            .clone()
            .oneshot(request(path, &fixture.token, body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path} stream={stream}");
        assert!(response.headers().get("set-cookie").is_none());
        let text = String::from_utf8(
            to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(text.contains("Hello"), "{path} stream={stream}: {text}");
        assert!(!text.contains("private-supplier"));
        assert!(!text.contains("fixture-project"));
        if !stream {
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(value["model"], "custom-model", "{text}");
        }
        assert_eq!(
            fixture
                .storage
                .user("user")
                .await
                .unwrap()
                .unwrap()
                .wallet_cents,
            98
        );
        let record: (String, Option<i64>, Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT status,input_tokens,output_tokens,reasoning_tokens,cost_nano_usd FROM usage_records",
        )
        .fetch_one(fixture.storage.pool())
        .await
        .unwrap();
        assert_eq!(
            record,
            (
                "completed".into(),
                Some(10000),
                Some(5000),
                Some(1000),
                Some(27_300_000)
            ),
            "{path} stream={stream}"
        );
        let public = fixture
            .storage
            .user_store()
            .usage("user", &codex2api_storage::UserUsageFilter::default())
            .await
            .unwrap();
        assert_eq!(public["items"][0]["upstream_endpoint"], "/v1/responses");
        let calls = fixture.mock.requests.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let (upstream_path, headers, envelope) = &calls[0];
        assert_eq!(upstream_path, "/v1internal:streamGenerateContent");
        assert!(
            headers["user-agent"]
                .to_str()
                .unwrap()
                .starts_with(&format!(
                    "antigravity/{} ",
                    codex2api_common::version::antigravity::VERSION
                ))
        );
        for name in [
            "originator",
            "chatgpt-account-id",
            "x-codex-installation-id",
            "cookie",
            "x-api-group",
            "openai-project",
            "x-codex-guardian",
            "x-xai-token-auth",
        ] {
            assert!(!headers.contains_key(name), "{name}");
        }
        assert_eq!(envelope["userAgent"], "antigravity");
        assert_eq!(envelope["requestType"], "agent");
        assert!(
            envelope["requestId"]
                .as_str()
                .unwrap()
                .starts_with("agent-")
        );
        assert_eq!(envelope["project"], "fixture-project");
        assert_eq!(envelope["model"], "custom-model");
        assert!(envelope.get("stream").is_none());
        let gemini = &envelope["request"];
        assert_eq!(gemini["contents"][0]["role"], "user");
        assert_eq!(gemini["contents"][0]["parts"][0]["text"], "Hello");
        assert_eq!(gemini["generationConfig"]["temperature"], 0.4);
        // Messages callers default to medium reasoning effort; its 8192-token
        // thinking budget raises the output limit to budget + 1000.
        let max_output: u64 = if path == "/v1/messages" {
            8192 + 1000
        } else {
            123
        };
        assert_eq!(
            gemini["generationConfig"]["maxOutputTokens"], max_output,
            "{path}"
        );
        assert!(
            gemini["systemInstruction"]["parts"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Antigravity")
        );
        assert!(gemini["sessionId"].as_str().unwrap().starts_with('-'));
        assert!(gemini.get("metadata").is_none());
    }

    // Explicit quota exhaustion cools down the exhausted model alone; a shared
    // capacity shortage is forwarded without touching supplier state.
    let fixture = fixture("antigravity").await;
    let generate = || {
        request(
            "/v1/responses",
            &fixture.token,
            json!({"model":"custom-model","input":"Hello"}),
        )
    };
    *fixture.mock.status.lock().unwrap() = Some(429);
    let response = fixture.router.clone().oneshot(generate()).await.unwrap();
    let status = response.status();
    let value = json_body(response).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{value}");
    assert_eq!(value["error"]["code"], "service_unavailable");
    assert!(!value.to_string().contains("private-supplier"));
    let cooldowns = fixture
        .storage
        .supplier_model_blocks("supplier")
        .await
        .unwrap();
    assert_eq!(cooldowns.len(), 1);
    assert_eq!(cooldowns[0].model, "custom-model");
    assert_eq!(
        cooldowns[0].code.clone().as_deref(),
        Some("quota_exhausted")
    );
    assert!(cooldowns[0].until_at.unwrap() > chrono::Utc::now().timestamp() + 1700);
    let health = fixture.storage.supplier_health("supplier").await.unwrap();
    assert!(
        health
            .account_block()
            .and_then(|block| block.until_at)
            .is_none()
    );
    assert!(!health.authentication_invalid());
    assert!(!health.payment_required());
    assert_eq!(fixture.mock.requests.lock().unwrap().len(), 1);
    let response = fixture.router.clone().oneshot(generate()).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(fixture.mock.requests.lock().unwrap().len(), 1);
    fixture
        .storage
        .unblock_supplier_model(
            "supplier",
            fixture
                .storage
                .supplier_auth_revision("supplier")
                .await
                .unwrap()
                .unwrap(),
            "custom-model",
            health.observation_revision,
        )
        .await
        .unwrap();
    *fixture.mock.status.lock().unwrap() = Some(503);
    let response = fixture.router.clone().oneshot(generate()).await.unwrap();
    let status = response.status();
    let value = json_body(response).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{value}");
    assert_eq!(value["error"]["code"], "service_unavailable", "{value}");
    assert!(
        fixture
            .storage
            .supplier_model_blocks("supplier")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(fixture.mock.requests.lock().unwrap().len(), 2);
    *fixture.mock.status.lock().unwrap() = None;
    let response = fixture.router.clone().oneshot(generate()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        fixture
            .storage
            .user("user")
            .await
            .unwrap()
            .unwrap()
            .wallet_cents,
        98
    );
}

#[tokio::test]
async fn antigravity_effort_variants_forward_only_the_reported_variant() {
    let fixture = fixture("antigravity").await;
    let revision = fixture
        .storage
        .supplier_auth_revision("supplier")
        .await
        .unwrap()
        .unwrap();
    assert!(
        fixture
            .storage
            .sync_supplier_models(
                "supplier",
                revision,
                &[
                    json!({"id":"custom-model","model":"custom-model","object":"model",
                    "name":"Custom Model","efforts":{"high":"custom-model-high","low":"custom-model-low"}})
                ],
            )
            .await
            .unwrap()
    );
    let generate = |effort: Option<&str>| {
        let mut body = json!({"model":"custom-model","input":"Hello"});
        if let Some(effort) = effort {
            body["reasoning"] = json!({"effort":effort});
        }
        request("/v1/responses", &fixture.token, body)
    };
    let response = fixture
        .router
        .clone()
        .oneshot(generate(Some("high")))
        .await
        .unwrap();
    let status = response.status();
    let value = json_body(response).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    assert_eq!(value["model"], "custom-model");
    assert_eq!(
        fixture.mock.requests.lock().unwrap()[0].2["model"],
        "custom-model-high"
    );
    // A missing or unreported effort is refused before anything reaches Antigravity.
    for (effort, refusal) in [
        (Some("medium"), "does not support reasoning effort 'medium'"),
        (None, "requires a reasoning effort"),
    ] {
        let response = fixture
            .router
            .clone()
            .oneshot(generate(effort))
            .await
            .unwrap();
        let status = response.status();
        let value = json_body(response).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{value}");
        let message = value["error"]["message"].as_str().unwrap();
        assert!(
            message.contains(refusal) && message.contains("low, high"),
            "{message}"
        );
    }
    assert_eq!(fixture.mock.requests.lock().unwrap().len(), 1);
}

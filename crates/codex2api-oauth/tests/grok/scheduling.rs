mod accounts {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
use accounts::AccountFixture;
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::{get, post},
};
use codex2api_storage::{NewSupplierAccount, PlatformAccount, Storage};
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tower::ServiceExt;

const FREE: &str = "You've used all the included free usage for model grok-4.7 for now. Usage resets over a rolling 24-hour window — tokens (actual/limit): 650951/600000. Upgrade to a Grok subscription for higher limits: https://grok.com/supergrok";

#[derive(Default)]
struct Mock {
    rejection: Mutex<Option<(u16, String)>>,
    calls: Mutex<Vec<String>>,
    accept_refreshed: AtomicBool,
}
impl codex2api_upstream::HttpTransport for Mock {
    fn send(&self, request: reqwest::RequestBuilder) -> codex2api_upstream::HttpFuture {
        let request = request.build().unwrap();
        let id = request.headers()["x-userid"].to_str().unwrap().to_owned();
        self.calls.lock().unwrap().push(id.clone());
        let refreshed = self.accept_refreshed.load(Ordering::Relaxed)
            && request.headers()["authorization"] == "Bearer refreshed-first";
        let reply = (id == "first" && !refreshed)
            .then(|| self.rejection.lock().unwrap().clone())
            .flatten();
        Box::pin(async move {
            let (status, body) = reply.unwrap_or_else(|| (200, json!({"object":"response","id":"resp_success","status":"completed","model":"grok-4.7","output":[],"usage":{"input_tokens":100,"output_tokens":20}}).to_string()));
            Ok(http::Response::builder()
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
                .header("x-should-retry", "false")
                .body(reqwest::Body::from(body))
                .unwrap()
                .into())
        })
    }
}

struct Fixture {
    _dir: codex2api_storage::test_support::TestDirectory,
    storage: Storage,
    app: Router,
    token: String,
    mock: Arc<Mock>,
}
async fn fixture() -> Fixture {
    fixture_with_issuer(None).await
}
async fn fixture_with_issuer(issuer: Option<&str>) -> Fixture {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("grok-scheduling-pg"))
        .await
        .unwrap();
    storage
        .sync_supported_models(&[codex2api_common::SupportedModel {
            platform: "grok".into(),
            model: "grok-4.7".into(),
            kind: "text".into(),
        }])
        .await
        .unwrap();
    let mut settings = storage.gateway_settings().await.unwrap();
    settings.scheduler_mode = "traditional".into();
    storage.save_gateway_settings(&settings).await.unwrap();
    let plan = storage.plan_fixture("grok").await.unwrap();
    let owner = PlatformAccount {
        provider_id: "grok".into(),
        id: "consumer".into(),
        username: "grok-user".into(),
        password_hash: codex2api_storage::hash_password("fixture-password").unwrap(),
        name: "Consumer".into(),
        email: "consumer@example.test".into(),
        plan_type: plan.plan_type,
        plan_id: plan.id,
        subscription_expires_at: None,
        enabled: true,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_account_fixture(&owner).await.unwrap();
    for (priority, id) in ["first", "second"].into_iter().enumerate() {
        let identity = codex2api_suppliers::grok::GrokIdentity::generate();
        let mut account = NewSupplierAccount::pending_identity(
            &identity.installation_id,
            &identity.os_type,
            &identity.os_version,
            &identity.arch,
            identity.fingerprint_json().unwrap(),
        );
        account.id = Some(id.into());
        account.provider_id = "grok".into();
        account.chatgpt_account_id = Some(format!("user:{id}"));
        account.chatgpt_user_id = Some(id.into());
        let credentials = codex2api_suppliers::grok::GrokCredentials {
            access_token: format!("token-{id}"),
            refresh_token: issuer.map(|_| format!("refresh-{id}")),
            id_token: None,
            expires_at: Some(chrono::Utc::now().timestamp() + 3600),
            profile: json!({"userId":id}),
        };
        storage
            .save_authorized_account(account, credentials.to_supplier_tokens(id), None)
            .await
            .unwrap();
        storage.add_supplier_model(id, "grok-4.7").await.unwrap();
        storage
            .save_supplier_scheduling(
                id,
                &codex2api_storage::SupplierScheduling {
                    priority: priority as i32,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    storage
        .save_supplier_tag("pool", "grok", "Pool")
        .await
        .unwrap();
    storage
        .edit_supplier_tags(&["first".into(), "second".into()], &["pool".into()], false)
        .await
        .unwrap();
    storage
        .save_pool_route(&owner.id, "grok", Some(&["pool".into()]), Some(1))
        .await
        .unwrap();
    let accounts = codex2api_suppliers::SupplierAccountStore::open(storage.clone());
    let auth = codex2api_suppliers::AuthService::new(accounts.clone()).unwrap();
    let mock = Arc::new(Mock::default());
    let mut upstream = codex2api_upstream::UpstreamPool::new(auth).with_transport(mock.clone());
    if let Some(issuer) = issuer {
        let grok = codex2api_suppliers::grok::GrokAuthService::new(Some(storage.clone()))
            .with_grok_config(codex2api_suppliers::grok::GrokConfig {
                issuer: issuer.into(),
                base_url: format!("{issuer}/v1"),
            });
        upstream = upstream.with_grok(codex2api_upstream::grok::GrokUpstream::new(grok));
    }
    let app = codex2api_oauth::router(
        codex2api_oauth::OAuthState::new(storage.clone(), accounts, upstream)
            .with_oauth_base_url("http://service.example.test")
            .unwrap(),
    );
    let users = codex2api_user::api_router(codex2api_user::UserState {
        storage: storage.user_store(),
        public_base_url: "http://service.example.test".into(),
        api_base_url: "http://127.0.0.1:8083".into(),
        api_concurrency: Arc::new(|_| 0),
    });
    let token = super::protocol::authorize(&app, &users).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    Fixture {
        _dir: dir,
        storage,
        app,
        token,
        mock,
    }
}

async fn send(f: &Fixture, endpoint: &str) -> axum::response::Response {
    let body = match endpoint {
        "responses" => json!({"model":"grok-4.7","input":"hi"}),
        _ => {
            json!({"model":"grok-4.7","messages":[{"role":"user","content":"hi"}],"max_tokens":64})
        }
    };
    f.app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/grok/v1/{endpoint}"))
                .header("authorization", format!("Bearer {}", f.token))
                .header("content-type", "application/json")
                .header("user-agent", "grok-pager/1.0.45")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn records(storage: &Storage) -> Vec<(String, String, Option<i64>, Option<i64>)> {
    for _ in 0..100 {
        let rows: Vec<(String,String,Option<i64>,Option<i64>)> = sqlx::query_as("SELECT account_id,status,input_tokens,output_tokens FROM usage_records ORDER BY requested_at_ms").fetch_all(storage.pool()).await.unwrap();
        if !rows.is_empty() && rows.iter().all(|row| row.1 != "in_progress") {
            return rows;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("request ledger did not settle");
}

#[tokio::test]
async fn oauth_grok_exhaustion_switches_http_and_sse_once_without_charging_rejections() {
    for (status, body) in [
        (429, json!({"code":429,"error":FREE}).to_string()),
        (
            200,
            format!(
                "data: {{\"type\":\"response.created\"}}\n\ndata: {}\n\n",
                json!({"type":"error","code":"subscription:free-usage-exhausted","error":FREE})
            ),
        ),
    ] {
        let f = fixture().await;
        *f.mock.rejection.lock().unwrap() = Some((status, body));
        let response = send(&f, "responses").await;
        assert_eq!(response.status(), StatusCode::OK);
        let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(*f.mock.calls.lock().unwrap(), ["first", "second"]);
        assert_eq!(
            records(&f.storage).await,
            [("second".into(), "completed".into(), Some(100), Some(20))]
        );
        let health = f.storage.supplier_health("first").await.unwrap();
        assert_eq!(
            health
                .account_block()
                .and_then(|block| block.code.clone())
                .as_deref(),
            Some("subscription:free-usage-exhausted")
        );
        assert!(!health.authentication_invalid());
        assert_eq!(health.display_status(true, true), "quota_exhausted");
        // An existing session must not return to its exhausted owner.
        let response = send(&f, "responses").await;
        assert_eq!(response.status(), StatusCode::OK);
        let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(*f.mock.calls.lock().unwrap(), ["first", "second", "second"]);
    }
}

#[tokio::test]
async fn oauth_grok_http_failures_with_usage_are_settled_without_replay() {
    for (endpoint, usage) in [
        (
            "responses",
            json!({"input_tokens":100,"output_tokens":20,"input_tokens_details":{"cached_tokens":40}}),
        ),
        (
            "chat/completions",
            json!({"prompt_tokens":100,"completion_tokens":20,"prompt_tokens_details":{"cached_tokens":40}}),
        ),
        (
            "messages",
            json!({"input_tokens":60,"cache_read_input_tokens":40,"output_tokens":20}),
        ),
    ] {
        let f = fixture().await;
        *f.mock.rejection.lock().unwrap() = Some((429, json!({"model":"grok-4.7","error":{"code":"subscription:free-usage-exhausted","message":FREE},"usage":usage}).to_string()));
        let response = send(&f, endpoint).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(*f.mock.calls.lock().unwrap(), ["first"]);
        assert_eq!(
            records(&f.storage).await,
            [("first".into(), "failed".into(), Some(100), Some(20))]
        );
        assert_eq!(
            f.storage
                .supplier_health("first")
                .await
                .unwrap()
                .quota_exhausted()
                .then_some("quota_exhausted")
                .as_deref(),
            Some("quota_exhausted")
        );
    }
}

#[tokio::test]
async fn oauth_grok_permission_throttling_and_server_errors_do_not_switch() {
    let f = fixture().await;
    for (status, code, public_status) in [
        (403, "permission_denied", 503),
        (429, "slow_down", 429),
        (503, "overloaded_error", 503),
        (429, "new_unknown_error", 429),
    ] {
        f.mock.calls.lock().unwrap().clear();
        *f.mock.rejection.lock().unwrap() = Some((
            status,
            json!({"error":{"code":code,"message":"rejected"}}).to_string(),
        ));
        let response = send(&f, "responses").await;
        assert_eq!(response.status().as_u16(), public_status);
        if status == 429 {
            assert_eq!(response.headers()["retry-after"], "120");
            assert_eq!(response.headers()["x-should-retry"], "false");
        } else {
            assert!(!response.headers().contains_key("retry-after"));
            assert!(!response.headers().contains_key("x-should-retry"));
        }
        let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(*f.mock.calls.lock().unwrap(), ["first"]);
        assert!(
            f.storage
                .supplier_health("first")
                .await
                .unwrap()
                .account_block()
                .and_then(|block| block.until_at)
                .is_none()
        );
    }
}

#[tokio::test]
async fn oauth_grok_unrefreshable_auth_switches_without_reusing_rejected_identity() {
    let f = fixture().await;
    *f.mock.rejection.lock().unwrap() = Some((401, json!({"error":"invalid token"}).to_string()));
    let response = send(&f, "responses").await;
    assert_eq!(response.status(), StatusCode::OK);
    let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(*f.mock.calls.lock().unwrap(), ["first", "second"]);
    let health = f.storage.supplier_health("first").await.unwrap();
    assert!(health.authentication_invalid());
    assert!(
        health
            .account_block()
            .and_then(|block| block.until_at)
            .is_none()
    );
}

#[tokio::test]
async fn oauth_grok_refreshes_once_and_only_permanent_refresh_failures_evict() {
    for (token_status, code, accept, expected_calls, invalid) in [
        (200, "", true, vec!["first", "first"], false),
        (200, "", false, vec!["first", "first", "second"], true),
        (400, "invalid_grant", false, vec!["first", "second"], true),
        (401, "invalid_client", false, vec!["first", "second"], true),
        (429, "slow_down", false, vec!["first"], false),
        (503, "temporarily_unavailable", false, vec!["first"], false),
        (401, "unknown", false, vec!["first"], false),
    ] {
        let refreshed = Arc::new(AtomicUsize::new(0));
        let count = refreshed.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new().route("/oauth2/token", post(move || {
            count.fetch_add(1, Ordering::Relaxed);
            async move {
                let body = if token_status == 200 { json!({"access_token":"refreshed-first","refresh_token":"rotated-first","expires_in":3600}) } else { json!({"error":code}) };
                (StatusCode::from_u16(token_status).unwrap(), [("retry-after", "19"),("x-should-retry","true")], Json(body))
            }
        })).route("/v1/user", get(|| async { Json(json!({"userId":"first"})) }))
            .route("/v1/settings", get(|| async { Json(json!({})) }));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let f = fixture_with_issuer(Some(&issuer)).await;
        f.mock.accept_refreshed.store(accept, Ordering::Relaxed);
        *f.mock.rejection.lock().unwrap() = Some((401, json!({"error":"expired"}).to_string()));
        let response = send(&f, "responses").await;
        assert_eq!(
            response.status().as_u16(),
            if invalid || accept { 200 } else { 503 }
        );
        if !invalid && !accept {
            assert!(!response.headers().contains_key("retry-after"));
            assert!(!response.headers().contains_key("x-should-retry"));
        }
        let _ = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(
            *f.mock.calls.lock().unwrap(),
            expected_calls,
            "{token_status} {code}"
        );
        assert_eq!(refreshed.load(Ordering::Relaxed), 1);
        assert_eq!(
            f.storage
                .supplier_health("first")
                .await
                .unwrap()
                .authentication_invalid(),
            invalid
        );
        assert!(
            f.storage
                .supplier_health("first")
                .await
                .unwrap()
                .account_block()
                .and_then(|block| block.until_at)
                .is_none()
        );
        server.abort();
    }
}

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use codex2api_storage::{Storage, SubscriptionChange, User};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn request(
    app: &Router,
    path: &str,
    body: Option<Value>,
    cookie: Option<&str>,
    csrf: Option<&str>,
) -> axum::response::Response {
    let mut r = Request::builder()
        .method(if body.is_some() { "POST" } else { "GET" })
        .uri(path)
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        r = r.header("cookie", cookie);
    }
    if let Some(csrf) = csrf {
        r = r.header("x-csrf-token", csrf);
    }
    app.clone()
        .oneshot(
            r.body(
                body.map(|v| Body::from(v.to_string()))
                    .unwrap_or_else(Body::empty),
            )
            .unwrap(),
        )
        .await
        .unwrap()
}
async fn body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1000000).await.unwrap()).unwrap()
}
fn cookie(response: &axum::response::Response) -> String {
    response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .into()
}
async fn setup() -> (tempfile::TempDir, Storage, Router, User) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("access.sqlite"))
        .await
        .unwrap();
    let user = User {
        id: "alice-user".into(),
        username: "alice".into(),
        password_hash: codex2api_storage::hash_password("user-password").unwrap(),
        name: "Alice User".into(),
        email: "alice@example.test".into(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_user(&user, None).await.unwrap();
    let mut other = user.clone();
    other.id = "bob-user".into();
    other.username = "bob".into();
    other.name = "Bob private".into();
    other.email = "bob-private@example.test".into();
    storage.save_user(&other, None).await.unwrap();
    for u in [&user, &other] {
        storage
            .save_user_subscription(SubscriptionChange {
                reissue: false,
                user_id: &u.id,
                plan_id: "plus",
                expires_at: None,
                enabled: true,
                revision: Some(1),
            })
            .await
            .unwrap();
    }
    let app = codex2api_user::router(codex2api_user::UserState {
        storage: storage.user_store(),
        public_base_url: "http://127.0.0.1:8082".into(),
    });
    (dir, storage, app, user)
}
fn query() -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("client_id", codex2api_version::OAUTH_CLIENT_ID),
            ("response_type", "code"),
            ("redirect_uri", "http://127.0.0.1:1455/auth/callback"),
            ("state", "exact-state"),
            ("code_challenge_method", "S256"),
            ("code_challenge", &"A".repeat(43)),
        ])
        .finish()
}

#[tokio::test]
async fn orders_show_exact_quote_and_require_owner_session_csrf_and_server_amount() {
    let (_dir, storage, app, user) = setup().await;
    let subscription = storage
        .user_subscriptions(Some(&user.id), true)
        .await
        .unwrap()
        .remove(0);
    storage
        .save_user_subscription(SubscriptionChange {
            reissue: false,
            user_id: &user.id,
            plan_id: "free",
            expires_at: None,
            enabled: true,
            revision: Some(subscription.revision),
        })
        .await
        .unwrap();
    storage
        .save_supplier_tag("private-pool", "chatgpt", "Hidden supplier pool")
        .await
        .unwrap();
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["sale_price_usd"] = json!("12.34");
    plan.config["duration_days"] = json!(30);
    plan.config["supplier_tag_id"] = json!("private-pool");
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    let subscription = storage
        .user_subscriptions(Some(&user.id), true)
        .await
        .unwrap()
        .remove(0);
    sqlx::query("UPDATE users SET wallet_cents=5000 WHERE id=?")
        .bind(&user.id)
        .execute(storage.pool())
        .await
        .unwrap();
    let response = request(
        &app,
        "/user/api/login",
        Some(json!({"username":"alice","password":"user-password"})),
        None,
        None,
    )
    .await;
    let alice_cookie = cookie(&response);
    let session = body(response).await;
    let csrf = session["csrf_token"].as_str().unwrap();
    let selection = json!({"plan_id":"plus","plan_revision":plan.revision,"subscription_revision":subscription.revision,"payment_method":"wallet","coupon_code":""});
    let preview_response = request(
        &app,
        "/user/api/checkout/preview",
        Some(selection),
        Some(&alice_cookie),
        Some(csrf),
    )
    .await;
    assert_eq!(preview_response.status(), StatusCode::OK);
    let preview = body(preview_response).await;
    let empty =
        body(request(&app, "/user/api/orders", None, Some(&alice_cookie), None).await).await;
    assert_eq!(empty["total"], 0);
    let input = json!({"preview_token":preview["preview_token"]});
    assert_eq!(
        request(
            &app,
            "/user/api/orders",
            Some(input.clone()),
            Some(&alice_cookie),
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let mut forged = input.clone();
    forged["amount_cents"] = json!(1);
    assert_eq!(
        request(
            &app,
            "/user/api/orders",
            Some(forged),
            Some(&alice_cookie),
            Some(csrf)
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let response = request(
        &app,
        "/user/api/orders",
        Some(input),
        Some(&alice_cookie),
        Some(csrf),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let order = body(response).await;
    let id = order["id"].as_str().unwrap();
    assert_eq!(order["amount_cents"], 1234);
    assert_eq!(order["status"], "pending");
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        5000
    );
    let response = request(
        &app,
        "/user/api/login",
        Some(json!({"username":"bob","password":"user-password"})),
        None,
        None,
    )
    .await;
    let bob_cookie = cookie(&response);
    let bob = body(response).await;
    assert_eq!(
        request(
            &app,
            &format!("/user/api/orders/{id}"),
            None,
            Some(&bob_cookie),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &app,
            &format!("/user/api/orders/{id}/pay"),
            Some(json!({})),
            Some(&bob_cookie),
            bob["csrf_token"].as_str()
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let paid = body(
        request(
            &app,
            &format!("/user/api/orders/{id}/pay"),
            Some(json!({})),
            Some(&alice_cookie),
            Some(csrf),
        )
        .await,
    )
    .await;
    assert_eq!(paid["status"], "paid");
    assert_eq!(paid["balance_cents"], 3766);
    let listed =
        body(request(&app, "/user/api/orders", None, Some(&alice_cookie), None).await).await;
    assert_eq!(listed["total"], 1);
    for secret in [
        "private-pool",
        "Hidden supplier",
        "request_signature",
        "username",
        "user_name",
    ] {
        assert!(!listed.to_string().contains(secret));
    }
    storage.close().await;
}
#[tokio::test]
async fn same_origin_dev_proxy_login_is_allowed_but_cross_site_login_is_rejected() {
    let (_dir, storage, app, _) = setup().await;
    for (site, origin, expected) in [
        ("same-origin", "http://127.0.0.1:3001", StatusCode::OK),
        (
            "cross-site",
            "http://attacker.invalid",
            StatusCode::FORBIDDEN,
        ),
        ("same-site", "http://127.0.0.1:3000", StatusCode::FORBIDDEN),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::post("/user/api/login")
                    .header("content-type", "application/json")
                    .header("origin", origin)
                    .header("sec-fetch-site", site)
                    .body(Body::from(
                        json!({"username":"alice","password":"user-password"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{site} {origin}");
        if expected == StatusCode::OK {
            let token = cookie(&response).split_once('=').unwrap().1.to_owned();
            assert!(
                storage
                    .verify_jwt(codex2api_storage::TokenPurpose::UserSession, &token)
                    .await
                    .is_ok()
            );
        }
    }
    storage.close().await;
}
#[tokio::test]
async fn user_reads_are_owner_scoped_and_mutations_require_their_browser_csrf() {
    let (_dir, storage, app, user) = setup().await;
    assert_eq!(
        request(
            &app,
            "/user/api/login",
            Some(json!({"username":"admin","password":"admin"})),
            None,
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = request(
        &app,
        "/user/api/login",
        Some(json!({"username":"alice","password":"user-password"})),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let session_cookie = cookie(&response);
    let session = body(response).await;
    let csrf = session["csrf_token"].as_str().unwrap();
    for path in [
        "/user/api/session",
        "/user/api/plans",
        "/user/api/wallet",
        "/user/api/subscriptions?user_id=bob-user",
        "/user/api/devices",
    ] {
        let response = request(&app, path, None, Some(&session_cookie), None).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let text = body(response).await.to_string();
        for secret in [
            "bob-user",
            "bob-private",
            "password_hash",
            "token_hash",
            "supplier_account_id",
            "supplier_tag_id",
            "installation_id",
        ] {
            assert!(!text.contains(secret), "{path}: {secret}");
        }
    }
    for path in [
        "/admin/api/suppliers",
        "/user/api/users",
        "/user/api/suppliers",
        "/user/api/config",
        "/user/api/users/bob-user",
    ] {
        assert_eq!(
            request(&app, path, None, Some(&session_cookie), None)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        request(
            &app,
            "/user/api/logout",
            Some(json!({})),
            Some(&session_cookie),
            Some("wrong")
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let response = app
        .clone()
        .oneshot(
            Request::get("/user/api/session")
                .header("cookie", &session_cookie)
                .header("authorization", "Bearer ai-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .clone()
        .oneshot(
            Request::post("/user/api/logout")
                .header("cookie", &session_cookie)
                .header("x-csrf-token", csrf)
                .header("origin", "http://127.0.0.1:8081")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(request(&app,"/user/api/password",Some(json!({"current_password":"user-password","new_password":"new-password","wallet_cents":99999})),Some(&session_cookie),Some(csrf)).await.status(),StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        0
    );
    assert_eq!(
        request(
            &app,
            "/user/api/logout",
            Some(json!({})),
            Some(&session_cookie),
            Some(csrf)
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        request(&app, "/user/api/session", None, Some(&session_cookie), None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    storage.close().await;
}
#[tokio::test]
async fn temporary_authorization_needs_explicit_confirmation_and_does_not_log_the_browser_in() {
    let (_dir, storage, app, user) = setup().await;
    let response = request(
        &app,
        &format!("/user/api/oauth/authorize/bootstrap?{}", query()),
        None,
        None,
        None,
    )
    .await;
    let flow_cookie = cookie(&response);
    let flow = body(response).await;
    assert!(flow["identity"].is_null());
    let account = storage
        .user_platform_account(&user.id, "chatgpt")
        .await
        .unwrap()
        .unwrap();
    let mut approval = json!({"request_id":flow["request_id"],"csrf_token":flow["csrf_token"],"account_id":account.id,"confirmed":true});
    assert_eq!(
        request(
            &app,
            "/user/api/oauth/authorize/approve",
            Some(approval.clone()),
            Some(&flow_cookie),
            None
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let response=request(&app,"/user/api/oauth/authorize/identify",Some(json!({"request_id":flow["request_id"],"csrf_token":flow["csrf_token"],"kind":"user","username":"alice","password":"user-password"})),Some(&flow_cookie),None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!response.headers().contains_key("set-cookie"));
    let identity = body(response).await;
    assert_eq!(identity["identity"]["username"], "alice");
    assert!(
        storage
            .virtual_devices(&account.id)
            .await
            .unwrap()
            .is_empty()
    );
    approval["confirmed"] = false.into();
    assert_eq!(
        request(
            &app,
            "/user/api/oauth/authorize/approve",
            Some(approval.clone()),
            Some(&flow_cookie),
            None
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    approval["confirmed"] = true.into();
    approval["account_id"] = "bob-account".into();
    assert_eq!(
        request(
            &app,
            "/user/api/oauth/authorize/approve",
            Some(approval.clone()),
            Some(&flow_cookie),
            None
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    approval["account_id"] = account.id.into();
    let response = request(
        &app,
        "/user/api/oauth/authorize/approve",
        Some(approval.clone()),
        Some(&flow_cookie),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let callback = body(response).await;
    let url = url::Url::parse(callback["redirect_uri"].as_str().unwrap()).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert_eq!(
        url.query_pairs().find(|(k, _)| k == "state").unwrap().1,
        "exact-state"
    );
    assert_eq!(
        request(
            &app,
            "/user/api/oauth/authorize/approve",
            Some(approval),
            Some(&flow_cookie),
            None
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM user_sessions")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    storage.close().await;
}
#[tokio::test]
async fn existing_browser_login_selects_user_but_logout_invalidates_unconfirmed_grants() {
    let (_dir, storage, app, _user) = setup().await;
    let response = request(
        &app,
        "/user/api/login",
        Some(json!({"username":"alice","password":"user-password"})),
        None,
        None,
    )
    .await;
    let session_cookie = cookie(&response);
    let session = body(response).await;
    let response = request(
        &app,
        &format!("/user/api/oauth/authorize/bootstrap?{}", query()),
        None,
        Some(&session_cookie),
        None,
    )
    .await;
    let flow_cookie = cookie(&response);
    let flow = body(response).await;
    assert_eq!(flow["identity"]["username"], "alice");
    let approval = json!({"request_id":flow["request_id"],"csrf_token":flow["csrf_token"],"account_id":flow["identity"]["account_id"],"confirmed":true});
    request(
        &app,
        "/user/api/logout",
        Some(json!({})),
        Some(&session_cookie),
        session["csrf_token"].as_str(),
    )
    .await;
    assert_eq!(
        request(
            &app,
            "/user/api/oauth/authorize/approve",
            Some(approval),
            Some(&flow_cookie),
            None
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    storage.close().await;
}

#[tokio::test]
async fn user_usage_filters_every_query_by_owner_and_never_serializes_supplier_data() {
    let (_dir, storage, app, user) = setup().await;
    let now = chrono::Utc::now().timestamp_millis();
    for (owner, id, input, output) in [
        ("alice-user", "own-request", 100, 40),
        ("bob-user", "other-request", 900, 800),
    ] {
        let account = storage
            .user_platform_account(owner, "chatgpt")
            .await
            .unwrap()
            .unwrap();
        let record = codex2api_storage::UsageRecord {
            id: id.into(),
            account_id: "supplier-secret-id".into(),
            account_name: "supplier-secret-name".into(),
            subject_id: account.id,
            subject_name: "private-label".into(),
            model: Some("gpt-5".into()),
            actual_model: Some("gpt-5".into()),
            endpoint: "/responses".into(),
            transport: "http".into(),
            status: "completed".into(),
            requested_at_ms: now,
            error_message: Some("supplier-secret-error".into()),
            upstream_request_id: Some("supplier-secret-request".into()),
            ..Default::default()
        };
        storage.insert_usage(&record).await.unwrap();
        sqlx::query("UPDATE usage_records SET input_tokens=?,output_tokens=?,cached_tokens=20,cost_nano_usd=1000000,billing_status='priced' WHERE id=?")
            .bind(input).bind(output).bind(id).execute(storage.pool()).await.unwrap();
    }
    let response = request(
        &app,
        "/user/api/login",
        Some(json!({"username":user.username,"password":"user-password"})),
        None,
        None,
    )
    .await;
    let auth = cookie(&response);
    let value = body(
        request(
            &app,
            "/user/api/usage?days=7&limit=10&tz_offset=-480",
            None,
            Some(&auth),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(value["total"], 1);
    assert_eq!(value["summary"]["input_tokens"], 100);
    assert_eq!(value["summary"]["total_tokens"], 140);
    assert_eq!(value["rows"][0]["request_count"], 1);
    assert_eq!(value["items"][0]["id"], "own-request");
    for forbidden in [
        "supplier-secret",
        "other-request",
        "private-label",
        "account_id",
        "account_name",
        "subject_id",
        "error_message",
        "upstream_request_id",
    ] {
        assert!(!value.to_string().contains(forbidden), "{forbidden}");
    }
    assert_eq!(
        request(
            &app,
            "/user/api/usage?user_id=bob-user",
            None,
            Some(&auth),
            None
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&app, "/user/api/usage", None, None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&app, "/user/api/coupons", None, Some(&auth), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    storage.close().await;
}

#[tokio::test]
async fn empty_usage_has_a_stable_shape_in_both_timezone_directions() {
    let (_dir, storage, app, _user) = setup().await;
    let response = request(
        &app,
        "/user/api/login",
        Some(json!({"username":"alice","password":"user-password"})),
        None,
        None,
    )
    .await;
    let auth = cookie(&response);
    for offset in [-840, -480, 0, 330, 840] {
        let response = request(
            &app,
            &format!("/user/api/usage?days=7&limit=10&tz_offset={offset}"),
            None,
            Some(&auth),
            None,
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "timezone offset {offset}"
        );
        let value = body(response).await;
        assert_eq!(value["summary"]["request_count"], 0);
        assert_eq!(value["summary"]["total_tokens"], 0);
        assert_eq!(value["summary"]["cost_nano_usd"], 0);
        assert_eq!(value["total"], 0);
        assert_eq!(value["rows"], json!([]));
        assert_eq!(value["items"], json!([]));
    }
    storage.close().await;
}

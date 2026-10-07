#[cfg(test)]
mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
#[cfg(test)]
use account_fixture::AccountFixture;
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use codex2api_storage::{PlatformAccount, Storage};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

async fn request(
    app: &Router,
    path: &str,
    body: Option<Value>,
    cookie: Option<&str>,
    bearer: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method(if body.is_some() { "POST" } else { "GET" })
        .uri(path)
        .header("host", "service.example.test")
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    if let Some(bearer) = bearer {
        request = request.header("authorization", format!("Bearer {bearer}"));
    }
    app.clone()
        .oneshot(
            request
                .body(
                    body.map(|v| Body::from(v.to_string()))
                        .unwrap_or_else(Body::empty),
                )
                .unwrap(),
        )
        .await
        .unwrap()
}
async fn payload(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap()).unwrap()
}

async fn authorize(app: &Router) -> Value {
    let verifier = "a".repeat(64);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("client_id", codex2api_version::grok::CLIENT_ID),
            ("response_type", "code"),
            ("redirect_uri", "http://127.0.0.1:43210/callback"),
            ("state", "client-state"),
            ("nonce", "client-nonce"),
            ("scope", codex2api_version::grok::SCOPE),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ])
        .finish();
    let bootstrap = request(
        app,
        &format!("/user/api/oauth/authorize/bootstrap?{query}"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(bootstrap.status(), StatusCode::OK);
    let cookie = bootstrap.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let flow = payload(bootstrap).await;
    assert_eq!(flow["provider_id"], "grok");
    let identified=request(app,"/user/api/oauth/authorize/identify",Some(json!({"request_id":flow["request_id"],"csrf_token":flow["csrf_token"],"username":"grok-user","password":"fixture-password"})),Some(&cookie),None).await;
    assert_eq!(identified.status(), StatusCode::OK);
    let identity = payload(identified).await;
    let approval=request(app,"/user/api/oauth/authorize/approve",Some(json!({"request_id":flow["request_id"],"csrf_token":flow["csrf_token"],"confirmed":true,"account_id":identity["identity"]["account_id"]})),Some(&cookie),None).await;
    assert_eq!(approval.status(), StatusCode::OK);
    let target = payload(approval).await;
    let target = url::Url::parse(target["redirect_uri"].as_str().unwrap()).unwrap();
    let code = target
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    let input = json!({"client_id":codex2api_version::grok::CLIENT_ID,"grant_type":"authorization_code","code":code,"redirect_uri":"http://127.0.0.1:43210/callback","code_verifier":verifier});
    let tokens = request(app, "/grok/oauth2/token", Some(input.clone()), None, None).await;
    assert_eq!(tokens.status(), StatusCode::OK);
    assert_eq!(
        request(app, "/grok/oauth2/token", Some(input), None, None)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    payload(tokens).await
}

#[tokio::test]
async fn subscription_names_match_grok_build_readers_and_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("subscription-names.sqlite"))
        .await
        .unwrap();
    let mut plan = storage.platform_free_plan("grok").await.unwrap();
    plan.id = "local-heavy".into();
    plan.name = "本地开发套餐".into();
    plan.plan_type = "supergrok_heavy".into();
    storage.save_virtual_plan(&plan, None).await.unwrap();
    let mut account = PlatformAccount {
        provider_id: "grok".into(),
        id: "subscription-user".into(),
        username: "grok-user".into(),
        password_hash: codex2api_storage::hash_password("fixture-password").unwrap(),
        name: "Local User".into(),
        email: "local@example.test".into(),
        plan_id: plan.id.clone(),
        plan_type: plan.plan_type.clone(),
        subscription_expires_at: None,
        enabled: true,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_account_fixture(&account).await.unwrap();
    let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
    let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
    let pool = codex2api_upstream::UpstreamPool::new(auth);
    let app = codex2api_api::router(
        codex2api_api::ApiState::new(storage.clone(), accounts, pool)
            .with_public_base_url("http://service.example.test")
            .unwrap(),
    )
    .merge(codex2api_user::router(codex2api_user::UserState {
        storage: storage.user_store(),
        public_base_url: "http://service.example.test".into(),
    }));
    let tokens = authorize(&app).await;
    let access = tokens["access_token"].as_str().unwrap();
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(access.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(claims["tier"], 5);
    let user = payload(request(&app, "/grok/v1/user", None, None, Some(access)).await).await;
    assert_eq!(user["subscriptionTier"], "SuperGrokPro");
    assert_eq!(user["email"], "local@example.test");
    let settings =
        payload(request(&app, "/grok/v1/settings", None, None, Some(access)).await).await;
    assert_eq!(settings["subscription_tier"], "supergrok_heavy");
    assert_eq!(settings["subscription_tier_display"], "SuperGrok Heavy");
    assert_eq!(
        storage.virtual_plan(&plan.id).await.unwrap().unwrap().name,
        "本地开发套餐"
    );

    account.subscription_expires_at = Some("2020-01-01T00:00:00Z".into());
    storage.save_account_fixture(&account).await.unwrap();
    let settings =
        payload(request(&app, "/grok/v1/settings", None, None, Some(access)).await).await;
    assert_eq!(settings["subscription_tier_display"], "Free");
    storage
        .save_public_url_settings(&codex2api_storage::PublicUrlSettings {
            api_url: "https://public.example.test:9443".into(),
            user_url: "http://users.example.test:8082".into(),
            admin_url: "https://admin.example.test".into(),
            revision: 0,
        })
        .await
        .unwrap()
        .unwrap();
    let renewed = payload(
        request(
            &app,
            "/grok/oauth2/token",
            Some(json!({
                "grant_type":"refresh_token", "client_id":codex2api_version::grok::CLIENT_ID,
                "refresh_token":tokens["refresh_token"]
            })),
            None,
            None,
        )
        .await,
    )
    .await;
    let access = renewed["access_token"].as_str().unwrap();
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(access.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(claims["tier"], 0);
    assert_eq!(claims["iss"], "https://public.example.test:9443/grok");
    let identity: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(
                renewed["id_token"]
                    .as_str()
                    .unwrap()
                    .split('.')
                    .nth(1)
                    .unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(identity["iss"], claims["iss"]);
}

#[tokio::test]
async fn native_login_dynamic_models_inference_billing_refresh_and_provider_isolation() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("protocol.sqlite"))
        .await
        .unwrap();
    let mut free = storage.platform_free_plan("grok").await.unwrap();
    free.config = json!({"model_access":"all","models":[],"spending_windows":[{"duration_seconds":2592000,"cost_limit_usd":"10"}]});
    storage
        .save_virtual_plan(&free, Some(free.revision))
        .await
        .unwrap();
    let account = PlatformAccount {
        provider_id: "grok".into(),
        id: uuid::Uuid::new_v4().to_string(),
        username: "grok-user".into(),
        password_hash: codex2api_storage::hash_password("fixture-password").unwrap(),
        name: "Local Grok User".into(),
        email: "local@example.test".into(),
        plan_id: free.id.clone(),
        plan_type: "free".into(),
        subscription_expires_at: None,
        enabled: true,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_account_fixture(&account).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let mock=Router::new()
        .route("/oauth2/token",post(||async{Json(json!({"access_token":"supplier-secret","refresh_token":"rotated-secret","expires_in":3600}))}))
        .route("/v1/user",get(||async{Json(json!({"userId":"supplier-user","email":"supplier@example.test"}))}))
        .route("/v1/models",get(||async{Json(json!({"data":[
            {"id":"grok-4.7","contextWindow":500000,"apiBackend":"responses","baseUrl":"https://private-supplier.example","_meta":{"access_token":"must-not-leak"}},
            {"id":"future-grok-model","context_window":1200000,"api_backend":"messages"}
        ]}))}))
        .route("/v1/responses",post(|headers:HeaderMap,Json(body):Json<Value>|async move {
            assert_eq!(headers["authorization"],"Bearer supplier-secret");assert_eq!(headers["x-xai-token-auth"],"xai-grok-cli");assert_eq!(headers["x-userid"],"supplier-user");
            assert!(headers.get("chatgpt-account-id").is_none());assert!(headers.get("originator").is_none());
            assert_eq!(body["store"],false);assert_eq!(headers["x-grok-model-override"],body["model"].as_str().unwrap());
            Json(json!({"object":"response","id":"response-fixture","status":"completed","model":body["model"],"output":[],"usage":{"input_tokens":100,"output_tokens":20,"input_tokens_details":{"cached_tokens":60},"output_tokens_details":{"reasoning_tokens":5}},"metadata":{"userId":"supplier-user","cost_in_usd_ticks":1234}}))
        }));
    let server = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    let grok = codex2api_auth::grok::GrokAuthService::new(Some(storage.clone())).with_grok_config(
        codex2api_auth::grok::GrokConfig {
            issuer: base.clone(),
            base_url: format!("{base}/v1"),
        },
    );
    let supplier = grok
        .grok_login_rt(
            codex2api_accounts::providers::grok::GrokIdentity::generate(),
            None,
            None,
            "fixture-rt",
        )
        .await
        .unwrap();
    let upstream = codex2api_upstream::grok::GrokUpstream::new(grok);
    let discovered = upstream.sync_models(&supplier.id).await.unwrap();
    assert_eq!(discovered["items"].as_array().unwrap().len(), 2);
    assert!(!discovered.to_string().contains("must-not-leak"));
    assert!(!discovered.to_string().contains("private-supplier"));
    assert!(
        !storage
            .model_config("grok", "future-grok-model")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
    let prices = storage.model_prices("grok").await.unwrap();
    let custom_model = "operator-only-model-91e8";
    assert!(codex2api_core::model_price_preset("grok", custom_model).is_none());
    assert!(
        !storage
            .grok_model_descriptors()
            .await
            .unwrap()
            .iter()
            .any(|m| m["model"] == custom_model)
    );
    let custom = codex2api_storage::ModelConfig {
        provider_id: "grok".into(),
        model: custom_model.into(),
        kind: "text".into(),
        enabled: true,
        deleted: false,
        revision: 0,
    };
    let mut custom_price = prices
        .iter()
        .find(|p| p.model == "grok-4.7" && p.min_input_tokens == 0)
        .unwrap()
        .clone();
    custom_price.model = custom_model.into();
    custom_price.source = "custom".into();
    custom_price.revision = 0;
    storage
        .save_model_config(&custom, &[custom_price], &[], None)
        .await
        .unwrap();

    assert!(
        prices
            .iter()
            .any(|p| p.model == "grok-4.7" && p.min_input_tokens == 200000)
    );
    storage
        .save_supplier_tag("grok-pool", "grok", "Grok pool")
        .await
        .unwrap();
    storage
        .replace_supplier_tags(&[supplier.id.clone()], &["grok-pool".into()])
        .await
        .unwrap();
    storage
        .save_pool_route(
            &account.id,
            "grok",
            Some("grok-pool"),
            Some(&supplier.id),
            None,
        )
        .await
        .unwrap();
    let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
    let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
    let pool = codex2api_upstream::UpstreamPool::new(auth).with_grok(upstream);
    let app = codex2api_api::router(
        codex2api_api::ApiState::new(storage.clone(), accounts, pool)
            .with_public_base_url("http://service.example.test")
            .unwrap(),
    )
    .merge(codex2api_user::router(codex2api_user::UserState {
        storage: storage.user_store(),
        public_base_url: "http://service.example.test".into(),
    }));
    let tokens = authorize(&app).await;
    let access = tokens["access_token"].as_str().unwrap();
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(
                tokens["id_token"]
                    .as_str()
                    .unwrap()
                    .split('.')
                    .nth(1)
                    .unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(claims["nonce"], "client-nonce");
    assert_eq!(claims["iss"], "http://service.example.test/grok");
    let models = payload(request(&app, "/grok/v1/models", None, None, Some(access)).await).await;
    assert!(
        models["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["model"] == "grok-4.7")
    );
    assert!(
        models["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["model"] == custom_model)
    );
    let shared = payload(request(&app, "/v1/models", None, None, Some(access)).await).await;
    assert_eq!(shared, models);
    assert_eq!(
        request(&app, "/backend-api/codex/models", None, None, Some(access))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = request(
        &app,
        "/v1/responses",
        Some(json!({"model":"grok-4.7","input":"test","stream":false})),
        None,
        Some(access),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let response = payload(response).await;
    assert!(!response.to_string().contains("supplier-user"));
    assert_eq!(response["usage"]["input_tokens"], 100);
    let mut charged = None;
    for _ in 0..40 {
        charged = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT cost_nano_usd FROM usage_records WHERE subject_id=? AND status='completed'",
        )
        .bind(&account.id)
        .fetch_optional(storage.pool())
        .await
        .unwrap()
        .flatten();
        if charged.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(charged, Some(230000));
    let custom_response = request(
        &app,
        "/v1/responses",
        Some(json!({"model":custom_model,"input":"custom model request","stream":false})),
        None,
        Some(access),
    )
    .await;
    assert_eq!(custom_response.status(), StatusCode::OK);
    assert_eq!(payload(custom_response).await["model"], custom_model);

    let refresh = json!({"grant_type":"refresh_token","client_id":codex2api_version::grok::CLIENT_ID,"refresh_token":tokens["refresh_token"]});
    let fresh = payload(
        request(
            &app,
            "/grok/oauth2/token",
            Some(refresh.clone()),
            None,
            None,
        )
        .await,
    )
    .await;
    assert_ne!(fresh["refresh_token"], tokens["refresh_token"]);
    assert_eq!(
        request(&app, "/grok/oauth2/token", Some(refresh), None, None)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(request(&app,"/grok/oauth2/revoke",Some(json!({"token":fresh["refresh_token"],"client_id":codex2api_version::grok::CLIENT_ID})),None,None).await.status(),StatusCode::OK);
    assert_eq!(
        request(
            &app,
            "/grok/v1/models",
            None,
            None,
            fresh["access_token"].as_str()
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    let discovery = payload(
        request(
            &app,
            "/api/oauth/grok/.well-known/openid-configuration",
            None,
            None,
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        discovery["issuer"],
        "http://service.example.test/api/oauth/grok"
    );
    assert_eq!(
        discovery["token_endpoint"],
        "http://service.example.test/api/oauth/grok/oauth2/token"
    );
    let issued=payload(request(&app,"/api/oauth/grok/oauth2/device/code",Some(json!({"client_id":codex2api_version::grok::CLIENT_ID,"scope":"openid profile offline_access"})),None,None).await).await;
    let flow = request(
        &app,
        "/user/api/oauth/device/bootstrap?provider=grok",
        None,
        None,
        None,
    )
    .await;
    let cookie = flow.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let flow = payload(flow).await;
    let identified=payload(request(&app,"/user/api/oauth/device/identify",Some(json!({"request_id":flow["request_id"],"csrf_token":flow["csrf_token"],"username":"grok-user","password":"fixture-password"})),Some(&cookie),None).await).await;
    assert_eq!(request(&app,"/user/api/oauth/device/approve",Some(json!({"request_id":flow["request_id"],"csrf_token":flow["csrf_token"],"confirmed":true,"account_id":identified["identity"]["account_id"],"user_code":issued["user_code"]})),Some(&cookie),None).await.status(),StatusCode::OK);
    let device_tokens=payload(request(&app,"/api/oauth/grok/oauth2/token",Some(json!({"client_id":codex2api_version::grok::CLIENT_ID,"grant_type":"urn:ietf:params:oauth:grant-type:device_code","device_code":issued["device_code"]})),None,None).await).await;
    assert_eq!(device_tokens["scope"], "openid profile offline_access");
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(
                device_tokens["id_token"]
                    .as_str()
                    .unwrap()
                    .split('.')
                    .nth(1)
                    .unwrap(),
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(claims["iss"], "http://service.example.test/api/oauth/grok");
    assert_eq!(
        request(
            &app,
            "/api/oauth/grok/v1/responses",
            Some(json!({"model":custom_model,"input":"test"})),
            None,
            device_tokens["access_token"].as_str()
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    server.abort();
    storage.close().await;
}

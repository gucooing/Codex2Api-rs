use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use codex2api_storage::{
    Storage, SupplierAccountUpdate, SupplierStatus, UsageRecord, VirtualAccount,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tower::ServiceExt;
const ROOT: &str = "/api/oauth/chatgpt";

#[tokio::test]
async fn jwt_matches_verified_official_shapes_and_keeps_virtual_permissions_isolated() {
    use rsa::{
        RsaPrivateKey, RsaPublicKey,
        pkcs1v15::{Signature, VerifyingKey},
        pkcs8::DecodePrivateKey,
        signature::Verifier,
    };
    use serde_json::json;
    use std::io::Write;
    fn decode(token: &str, part: usize) -> Value {
        serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(token.split('.').nth(part).unwrap())
                .unwrap(),
        )
        .unwrap()
    }
    fn shape(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                Value::Object(map.iter().map(|(k, v)| (k.clone(), shape(v))).collect())
            }
            Value::Array(items) => json!(items.first().map(shape).into_iter().collect::<Vec<_>>()),
            Value::String(_) => json!("string"),
            Value::Bool(_) => json!("boolean"),
            Value::Number(v) => json!(if v.is_i64() { "integer" } else { "number" }),
            Value::Null => json!("null"),
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("jwt-contract.sqlite");
    let storage = Storage::open(&db).await.unwrap();
    let (app, account) = fixture(&storage).await;
    let supplier = test_supplier(&storage, &account).await.unwrap();
    let tokens = login(&app).await;
    let expected: Value = serde_json::from_str(include_str!("oauth_jwt_shape.json")).unwrap();
    let pem = storage.oauth_jwt_private_key().await.unwrap().unwrap();
    let public = RsaPublicKey::from(&RsaPrivateKey::from_pkcs8_pem(&pem).unwrap());
    let verify = VerifyingKey::<Sha256>::new(public);
    for key in ["access_token", "id_token"] {
        let token = tokens[key].as_str().unwrap();
        let parts: Vec<_> = token.split('.').collect();
        assert_eq!(shape(&decode(token, 0)), expected[key]["header"]);
        assert_eq!(shape(&decode(token, 1)), expected[key]["payload"]);
        assert_eq!(decode(token, 0)["alg"], "RS256");
        verify
            .verify(
                format!("{}.{}", parts[0], parts[1]).as_bytes(),
                &Signature::try_from(URL_SAFE_NO_PAD.decode(parts[2]).unwrap().as_slice()).unwrap(),
            )
            .unwrap();
    }
    let access = decode(tokens["access_token"].as_str().unwrap(), 1);
    let identity = decode(tokens["id_token"].as_str().unwrap(), 1);
    assert_eq!(access["aud"], json!(["https://api.openai.com/v1"]));
    assert_eq!(identity["aud"], json!([codex2api_version::OAUTH_CLIENT_ID]));
    assert_eq!(access["iss"], codex2api_version::OAUTH_ISSUER);
    assert_eq!(identity["iss"], access["iss"]);
    assert_eq!(
        access["exp"].as_i64().unwrap() - access["iat"].as_i64().unwrap(),
        864000
    );
    assert_eq!(
        identity["exp"].as_i64().unwrap() - identity["iat"].as_i64().unwrap(),
        3600
    );
    assert_eq!(tokens["expires_in"], 864000);
    assert_eq!(access["nbf"], access["iat"]);
    assert_eq!(
        access["scp"],
        json!(
            codex2api_version::OAUTH_SCOPE
                .split_whitespace()
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(
        identity["at_hash"],
        URL_SAFE_NO_PAD
            .encode(&Sha256::digest(tokens["access_token"].as_str().unwrap().as_bytes())[..16])
    );
    assert_eq!(identity["sid"], access["session_id"]);
    assert_eq!(identity["sub"], access["sub"]);
    assert_eq!(identity["auth_provider"], "password");
    assert_eq!(identity["acr"], "0");
    assert_eq!(identity["amr"], json!(["pwd", "urn:openai:amr:password"]));
    assert_eq!(access["https://api.openai.com/mfa"]["required"], "no");
    assert_eq!(identity["email_verified"], false);
    let owner = "https://api.openai.com/auth";
    assert_eq!(
        identity[owner]["chatgpt_subscription_active_until"],
        account.subscription_expires_at.as_deref().unwrap()
    );
    assert_eq!(access[owner]["chatgpt_account_id"], account.id);
    assert_eq!(
        identity[owner]["organizations"][0]["id"],
        access[owner]["poid"]
    );
    for forbidden in ["scope", "role", "provider", "token_use"] {
        assert!(access.get(forbidden).is_none());
        assert!(identity.get(forbidden).is_none());
    }
    // Neither an ID token nor a modified JWT is an access credential.
    for invalid in [
        tokens["id_token"].as_str().unwrap().to_owned(),
        format!("{}x", tokens["access_token"].as_str().unwrap()),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json(
                    "GET",
                    "/backend-api/wham/usage",
                    &invalid,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    // Verify true local timestamps, and ensure refreshing does not authenticate again.
    let device = storage
        .virtual_devices(&account.id)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(access["pwd_auth_time"], device.authenticated_at_ms.unwrap());
    assert_eq!(
        identity["auth_time"],
        device.authenticated_at_ms.unwrap() / 1000
    );
    assert_eq!(identity["rat"], device.requested_at_ms.unwrap() / 1000);
    // An older registered token remains valid until its own expiry/revocation.
    assert!(
        storage
            .register_virtual_access(
                &device.id,
                tokens["refresh_token"].as_str().unwrap(),
                "legacy-access",
                chrono::Utc::now().timestamp() + 3600
            )
            .await
            .unwrap()
    );
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/usage",
                "legacy-access",
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    storage
        .update_account(
            &supplier,
            SupplierAccountUpdate {
                plan_type: Some("enterprise".into()),
                email: Some("foreign-supplier@example.test".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut second = account.clone();
    second.id = uuid::Uuid::new_v4().to_string();
    second.username = "bob".into();
    second.name = "Bob Virtual".into();
    second.email = "bob@virtual.test".into();
    second.plan_id = "plus".into();
    second.plan_type = "plus".into();
    storage.save_virtual_account(&second).await.unwrap();
    bind_test_supplier(&storage, &second, Some(supplier)).await;
    storage
        .create_virtual_device(&second, "bob-refresh", &Default::default())
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/token"),
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", "bob-refresh"),
            ],
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bob = json_body(response).await;
    let bob_access = decode(bob["access_token"].as_str().unwrap(), 1);
    assert_eq!(bob_access[owner]["chatgpt_account_id"], second.id);
    assert_eq!(bob_access[owner]["chatgpt_plan_type"], "plus");
    assert_ne!(bob_access["sub"], access["sub"]);
    assert_ne!(bob_access[owner]["poid"], access[owner]["poid"]);
    assert!(!bob_access.to_string().contains(&account.email));
    bind_test_supplier(&storage, &account, None).await;
    storage.close().await;
    let storage = Storage::open(&db).await.unwrap();
    let app = router(&storage);
    let response = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/token"),
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", tokens["refresh_token"].as_str().unwrap()),
            ],
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let refreshed = json_body(response).await;
    let new_access = decode(refreshed["access_token"].as_str().unwrap(), 1);
    let new_id = decode(refreshed["id_token"].as_str().unwrap(), 1);
    assert_eq!(new_access[owner], access[owner]);
    assert_eq!(new_access["session_id"], access["session_id"]);
    assert_eq!(new_access["pwd_auth_time"], access["pwd_auth_time"]);
    assert_eq!(new_id["auth_time"], identity["auth_time"]);
    assert_eq!(new_id["rat"], identity["rat"]);
    assert_eq!(
        new_id["at_hash"],
        URL_SAFE_NO_PAD
            .encode(&Sha256::digest(refreshed["access_token"].as_str().unwrap().as_bytes())[..16])
    );
    assert_ne!(new_access["jti"], access["jti"]);
    assert_eq!(new_access[owner]["chatgpt_plan_type"], "pro");
    assert!(!new_access.to_string().contains("foreign-supplier"));
    if let Ok(asar) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        let sample = json!({"access_token":tokens["access_token"],"refreshed_access_token":refreshed["access_token"],"account_id":account.id,"user_id":format!("user-{}",account.id),"email":account.email,"plan_type":"pro"});
        let mut child = std::process::Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/windows/Test-DesktopJwtContract.cjs"),
            )
            .arg(asar)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sample.to_string().as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        println!("{}", String::from_utf8_lossy(&result.stdout));
    }
    // Disabling one virtual account revokes only its local sessions.
    let mut disabled = account.clone();
    disabled.enabled = false;
    storage.save_virtual_account(&disabled).await.unwrap();
    assert!(
        storage
            .virtual_access(&codex2api_storage::hash_token(
                refreshed["access_token"].as_str().unwrap()
            ))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .virtual_access(&codex2api_storage::hash_token(
                bob["access_token"].as_str().unwrap()
            ))
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn reset_credits_match_official_clients_and_clear_only_current_virtual_usage() {
    use serde_json::json;
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("reset-contract.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["primary_cost_limit_usd"] = json!(1);
    plan.config["weekly_cost_limit_usd"] = json!(2);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    storage
        .grant_virtual_reset_credits(&account.id, "grant", 2, "private administration only")
        .await
        .unwrap();
    let login = login(&app).await;
    let token = login["access_token"].as_str().unwrap();
    let mut record = UsageRecord {
        id: "reset-usage".into(),
        subject_id: account.id.clone(),
        model: Some("gpt-6-astra".into()),
        endpoint: "/v1/responses".into(),
        requested_at_ms: chrono::Utc::now().timestamp_millis(),
        status: "in_progress".into(),
        ..Default::default()
    };
    storage.insert_usage(&record).await.unwrap();
    record.input_tokens = Some(100000);
    record.output_tokens = Some(0);
    record.status = "completed".into();
    storage.finish_usage(&record).await.unwrap();
    let mut sample = json!({"account_id":account.id,"access_token":token,"generation":false,"extra_routes":{},"reset_cases":{}});
    for (key, path) in [
        ("workspace", "/backend-api/wham/accounts/check"),
        ("quota", "/backend-api/wham/usage"),
        ("models", "/backend-api/codex/models"),
        ("config", "/backend-api/wham/config/bundle"),
        ("settings", "/backend-api/wham/settings/user"),
        (
            "credits_before_reset",
            "/backend-api/wham/rate-limit-reset-credits",
        ),
    ] {
        let r = app
            .clone()
            .oneshot(client_json("GET", path, token, Value::Null))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK, "{path}");
        sample[key] = json_body(r).await;
    }
    assert_eq!(
        sample["quota"]["rate_limit_reset_credits"],
        json!({"available_count":2})
    );
    assert_eq!(sample["quota"]["rate_limit"]["allowed"], false);
    assert!(sample["quota"]["rate_limit"].get("windows").is_none());
    assert!(sample["quota"].get("billing").is_none());
    for key in ["primary_window", "secondary_window"] {
        let window = &sample["quota"]["rate_limit"][key];
        assert_eq!(window.as_object().unwrap().len(), 4);
        assert!(window["used_percent"].is_i64());
    }
    let cards = sample["credits_before_reset"].clone();
    assert_eq!(cards.as_object().unwrap().len(), 3);
    assert_eq!(cards["total_earned_count"], 2);
    assert!(!cards.to_string().contains("private administration only"));
    let card = cards["credits"][0]["id"].as_str().unwrap();
    // All official compatibility mounts return the same data, without a supplier request.
    for prefix in ["/wham", "/api/codex", "/v1/api/codex", "/v1/wham"] {
        let r = app
            .clone()
            .oneshot(
                Request::get(format!("{prefix}/rate-limit-reset-credits"))
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(json_body(r).await, cards);
    }
    let path = "/backend-api/wham/rate-limit-reset-credits/consume";
    for bad in [
        json!({}),
        json!({"redeem_request_id":""}),
        json!({"redeem_request_id":42}),
        json!({"redeem_request_id":"bad","credit_id":" "}),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json("POST", path, token, bad))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    for (label, body) in [
        (
            "reset-use",
            json!({"redeem_request_id":"reset-use","credit_id":card}),
        ),
        (
            "reset-retry",
            json!({"redeem_request_id":"reset-use","credit_id":card}),
        ),
        ("reset-empty", json!({"redeem_request_id":"reset-empty"})),
        (
            "reset-missing",
            json!({"redeem_request_id":"reset-missing","credit_id":"not-this-account"}),
        ),
    ] {
        let r = app
            .clone()
            .oneshot(client_json("POST", path, token, body))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        sample["reset_cases"][label] = json_body(r).await;
    }
    assert_eq!(sample["reset_cases"]["reset-use"]["code"], "reset");
    assert_eq!(sample["reset_cases"]["reset-use"]["windows_reset"], 2);
    assert_eq!(
        sample["reset_cases"]["reset-retry"]["code"],
        "already_redeemed"
    );
    assert_eq!(
        sample["reset_cases"]["reset-empty"]["code"],
        "nothing_to_reset"
    );
    assert_eq!(sample["reset_cases"]["reset-missing"]["code"], "no_credit");
    for (key, path) in [
        ("quota_after_reset", "/backend-api/wham/usage"),
        (
            "credits_after_reset",
            "/backend-api/wham/rate-limit-reset-credits",
        ),
    ] {
        let r = app
            .clone()
            .oneshot(client_json("GET", path, token, Value::Null))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        sample[key] = json_body(r).await;
    }
    assert_eq!(
        sample["quota_after_reset"]["rate_limit"]["primary_window"]["used_percent"],
        0
    );
    assert_eq!(sample["quota_after_reset"]["rate_limit"]["allowed"], true);
    assert_eq!(sample["credits_after_reset"]["available_count"], 1);
    assert_eq!(sample["quota_after_reset"]["account_id"], account.id);
    assert_eq!(
        storage
            .virtual_account(&account.id)
            .await
            .unwrap()
            .unwrap()
            .subscription_expires_at,
        account.subscription_expires_at
    );
    sample["extra_routes"]["/backend-api/wham/rate-limit-reset-credits"] =
        json!({"status":200,"body":cards});
    let script_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/windows");
    let mut commands = Vec::new();
    if let Ok(asar) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        let mut command = std::process::Command::new("node");
        command
            .arg(script_root.join("Test-DesktopResetCredits.cjs"))
            .arg(asar);
        commands.push(command);
    }
    for variable in [
        "CODEX2API_TEST_NATIVE_UPDATE",
        "CODEX2API_TEST_DESKTOP_NATIVE",
    ] {
        if let Ok(native) = std::env::var(variable) {
            let mut command = std::process::Command::new("python");
            command
                .arg(script_root.join("Test-NativeUpdateContract.py"))
                .env("CODEX2API_TEST_NATIVE_UPDATE", native)
                .env("PYTHONIOENCODING", "utf-8");
            commands.push(command);
        }
    }
    for mut command in commands {
        let mut child = command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sample.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
}

#[tokio::test]
async fn updated_workspace_quota_and_catalog_are_accepted_by_the_actual_native_client() {
    use serde_json::json;
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("native-update.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["primary_cost_limit_usd"] = json!(5);
    plan.config["weekly_cost_limit_usd"] = json!(10);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    let mut sample =
        json!({"account_id":account.id,"access_token":token,"generation":true,"extra_routes":{}});
    for (key, path) in [
        ("workspace", "/backend-api/wham/accounts/check"),
        ("quota", "/backend-api/wham/usage"),
        ("models", "/backend-api/codex/models"),
        ("config", "/backend-api/wham/config/bundle"),
        ("settings", "/backend-api/wham/settings/user"),
        ("plugins", "/backend-api/ps/plugins/installed"),
    ] {
        let response = app
            .clone()
            .oneshot(client_json("GET", path, token, Value::Null))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        sample[key] = json_body(response).await;
    }
    assert_eq!(
        sample["workspace"]["accounts"][0]["workspace_backend_origin"],
        "NO_CONSTRAINT"
    );
    assert_eq!(sample["workspace"]["accounts"][0]["id"], account.id);
    assert!(!sample["models"]["models"].as_array().unwrap().is_empty());
    assert!(sample["quota"]["rate_limit"]["primary_window"]["used_percent"].is_i64());
    if let Ok(archive) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        let mut child = std::process::Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/windows/Test-DesktopWorkspaceContract.cjs"),
            )
            .arg(archive)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sample["workspace"].to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for path in [
        "/backend-api/wham/rate-limit-reset-credits",
        "/backend-api/ps/plugins/suggested/codex",
        "/backend-api/plugins/featured",
    ] {
        let response = app
            .clone()
            .oneshot(client_json("GET", path, token, Value::Null))
            .await
            .unwrap();
        let status = response.status();
        assert!(
            matches!(status, StatusCode::OK | StatusCode::NOT_IMPLEMENTED),
            "{path}: {status}"
        );
        sample["extra_routes"][path] =
            json!({"status":status.as_u16(),"body":json_body(response).await});
    }
    if std::env::var_os("CODEX2API_TEST_NATIVE_UPDATE").is_some() {
        let mut child = std::process::Command::new("python")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/windows/Test-NativeUpdateContract.py"),
            )
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sample.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
}

#[tokio::test]
async fn model_policy_covers_token_inspection_realtime_and_native_preparation_before_forwarding() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("model-entrypoints.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["model_access"] = json!("selected");
    plan.config["models"] = json!([{"provider_id":"chatgpt","model":"gpt-6-astra"}]);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    for (path, body, status) in [
        (
            "/backend-api/wham/realtime/calls?intent=quicksilver&architecture=avas",
            json!({"sdp":"v=0","session":{"model":"not-entitled"}}),
            StatusCode::FORBIDDEN,
        ),
        (
            "/v1/responses/input_tokens",
            json!({"model":"not-entitled","input":[]}),
            StatusCode::FORBIDDEN,
        ),
        (
            "/v1/responses/input_tokens",
            json!({"model":{}}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/backend-api/codex/realtime/calls",
            json!({"sdp":"v=0","session":{"model":"not-entitled"}}),
            StatusCode::FORBIDDEN,
        ),
        (
            "/backend-api/codex/realtime/calls",
            json!({"sdp":"v=0","session":{}}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/backend-api/codex/realtime/calls?model=gpt-6-astra",
            json!({"sdp":"v=0","session":{"model":"not-entitled"}}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/backend-api/codex/realtime/calls?model=gpt-6-astra&model=not-entitled",
            json!({"sdp":"v=0","session":{"model":"gpt-6-astra"}}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/backend-api/codex/realtime/calls?call_id=foreign",
            json!({"sdp":"v=0","session":{"model":"gpt-6-astra"}}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/backend-api/codex/realtime/calls",
            json!({"sdp":"v=0","session":{"model":"gpt-6-astra","audio":{"input":{"transcription":{"model":"not-entitled"}}}}}),
            StatusCode::FORBIDDEN,
        ),
        (
            "/backend-api/codex/realtime/calls",
            json!({"sdp":"v=0","session":{"model":"gpt-6-astra","audio":{"input":{"transcription":{}}}}}),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/backend-api/f/conversation/prepare",
            json!({"model":"not-entitled"}),
            StatusCode::FORBIDDEN,
        ),
        (
            "/backend-api/f/conversation",
            json!({"model":"not-entitled"}),
            StatusCode::FORBIDDEN,
        ),
        (
            "/backend-api/wham/tasks",
            json!({"new_task":{},"input_items":[]}),
            StatusCode::NOT_IMPLEMENTED,
        ),
        (
            "/backend-api/wham/tasks",
            json!({"new_task":{},"metadata":false,"input_items":[]}),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let mut request = client_json("POST", path, token, body);
        if path.starts_with("/v1/") {
            *request.uri_mut() = path.parse().unwrap();
        }
        let response = app.clone().oneshot(request).await.unwrap();
        let actual = response.status();
        let output = text_body(response).await;
        assert_eq!(actual, status, "{path}: {output}");
    }
    let operations = storage
        .virtual_resources(&account.id, "task_operation")
        .await
        .unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0]["status"], "model_unavailable");
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["primary_cost_limit_usd"] = json!(5);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(client_json(
            "POST",
            "/backend-api/codex/realtime/calls",
            token,
            json!({"sdp":"v=0","session":{"model":"gpt-6-astra"}}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "model_pricing_unavailable"
    );
    assert_eq!(
        storage
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        0
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    for (query, status) in [
        ("call_id=foreign&model=gpt-6-astra", StatusCode::NOT_FOUND),
        (
            "call_id=a&call_id=b&model=gpt-6-astra",
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = client
            .get(format!("http://{addr}/v1/realtime?{query}"))
            .bearer_auth(token)
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{query}");
    }
    server.abort();
}

#[tokio::test]
async fn global_catalog_and_desktop_picker_agree_when_legacy_groups_omit_new_models() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("catalog-policy.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["model_access"] = json!("selected");
    plan.config["models"] = json!([{"provider_id":"chatgpt","model":"gpt-5.6-luna"},{"provider_id":"chatgpt","model":"gpt-6-astra"}]);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let old = storage.virtual_config(&account.id, "models").await.unwrap();
    let mut legacy = json!({"models":[{"slug":"gpt-6-astra","title":"Astra","description":""},{"slug":"denied","title":"Denied","description":""}],"versions":[{"id":"legacy","display_text":"Legacy","slugs":["gpt-6-astra","denied"],"intelligence_presets":[{"model_slug":"gpt-6-astra","title":"Astra","lane":"auto"},{"model_slug":"denied","title":"Denied","lane":"auto"}]}],"categories":[],"internal_groups":[{"id":"old","model_ids":["denied"]}],"slider_settings":[],"default_model_slug":"denied"});
    codex2api_storage::prepare_virtual_contract("models", &mut legacy);
    storage
        .update_virtual_config(&account.id, "models", &legacy, old.revision)
        .await
        .unwrap();
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    let catalog = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/models",
                token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(catalog["models"].as_array().unwrap().len(), 2);
    assert!(!catalog.to_string().contains("denied"));
    assert!(
        catalog["versions"][0]["intelligence_presets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["model_slug"] == "gpt-5.6-luna")
    );
    if let Ok(archive) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let mut child = Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/windows/Test-DesktopModelCatalog.cjs"),
            )
            .arg(archive)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(catalog.to_string().as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        println!("{}", String::from_utf8_lossy(&result.stdout));
    }
    let supplier = test_supplier(&storage, &account).await.unwrap();
    bind_test_supplier(&storage, &account, None).await;
    let codex = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/codex/models",
                token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(codex["models"].as_array().unwrap().len(), 2);
    assert!(
        codex["models"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["supported_reasoning_levels"].is_array())
    );
    bind_test_supplier(&storage, &account, Some(supplier)).await;
}

#[tokio::test]
async fn oauth_grants_are_consumer_only_scope_bounded_and_not_admin_credentials() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("least-privilege.sqlite"))
        .await
        .unwrap();
    let (api, account) = fixture(&storage).await;
    let app = api.merge(codex2api_admin::router(
        codex2api_admin::AdminState::new(storage.clone()).unwrap(),
    ));
    let tokens = login_scoped(&app, "openid profile offline_access").await;
    assert_eq!(tokens["scope"], "openid profile offline_access");
    let token = tokens["access_token"].as_str().unwrap();
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(token.split('.').nth(1).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(claims.get("role").is_none());
    assert!(claims.get("provider").is_none());
    assert!(claims.get("token_use").is_none());
    assert!(claims.get("scope").is_none());
    assert_eq!(
        claims["scp"],
        json!(["openid", "profile", "offline_access"])
    );
    assert!(claims.get("email").is_none());
    assert_eq!(
        claims["https://api.openai.com/auth"]["chatgpt_account_id"],
        account.id
    );
    let denied = app
        .clone()
        .oneshot(client_json(
            "POST",
            "/backend-api/ps/mcp",
            token,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"anything"}}),
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(denied).await["error"]["code"],
        "insufficient_scope"
    );
    for path in [
        "/admin/api/consumers",
        "/admin/api/plans",
        "/admin/api/models",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::get(path)
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let rejected = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/token"),
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", tokens["refresh_token"].as_str().unwrap()),
                ("scope", codex2api_version::OAUTH_SCOPE),
            ],
            None,
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(rejected).await["error"], "invalid_scope");
    let narrow = login_scoped(&app, "openid").await;
    assert!(narrow.get("refresh_token").is_none());
    let connector_only = login_scoped(&app, "api.connectors.read").await;
    assert!(connector_only.get("id_token").is_none());
    assert_eq!(connector_only["scope"], "api.connectors.read");
}

#[tokio::test]
async fn consumer_tokens_cannot_cross_account_headers_paths_queries_or_resources() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("oauth-isolation.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut other = account.clone();
    other.id = "other-consumer".into();
    other.username = "other-consumer".into();
    storage.save_virtual_account(&other).await.unwrap();
    storage
        .save_virtual_resource(
            &other.id,
            "conversation",
            "private-conversation",
            None,
            &json!({"id":"private-conversation","title":"private"}),
        )
        .await
        .unwrap();
    for path in [
        format!("/backend-api/accounts/{}/settings", other.id),
        format!(
            "/backend-api/models?account_id={}&account_id={}",
            account.id, other.id
        ),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json("GET", &path, &token, Value::Null))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    let request = Request::get(format!("{ROOT}/backend-api/me"))
        .header("authorization", format!("Bearer {token}"))
        .header("chatgpt-account-id", &other.id)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/conversation/private-conversation",
                &token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let legacy = app
        .oneshot(
            Request::post("/v1/responses")
                .header("authorization", "Bearer c2a_retired_key")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(legacy.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        storage
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        0
    );
}

#[tokio::test]
async fn custom_named_plan_model_checkboxes_control_client_catalog_and_request_permissions() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("plan-models.sqlite"))
        .await
        .unwrap();
    let (api, mut account) = fixture(&storage).await;
    let app = api.merge(codex2api_admin::router(
        codex2api_admin::AdminState::new(storage.clone()).unwrap(),
    ));
    let (cookie, csrf) = admin_login(&app).await;
    let mut input = json!({"provider_id":"chatgpt","name":"研发专用","model_access":"selected","models":[{"provider_id":"chatgpt","model":"gpt-6-astra"}],"free_model_access":"none","free_models":[],"free_access_enabled":false,"primary_cost_limit_usd":"2","weekly_cost_limit_usd":"10","free_primary_cost_limit_usd":"0","free_weekly_cost_limit_usd":"0","enabled":true});
    let saved = app
        .clone()
        .oneshot(admin_request(
            "POST",
            "/admin/api/plans",
            &cookie,
            &csrf,
            input.clone(),
        ))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    let plan = storage
        .virtual_plans()
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.name == "研发专用")
        .unwrap();
    account.plan_id = plan.id.clone();
    account.plan_type = plan.plan_type.clone();
    storage
        .save_virtual_account_operation(&account, "admin")
        .await
        .unwrap();
    let catalog = storage.virtual_config(&account.id, "models").await.unwrap();
    let mut value = catalog.value;
    value["models"] = json!([{"slug":"gpt-6-astra"},{"slug":"gpt-5.6-luna"}]);
    storage
        .update_virtual_config(&account.id, "models", &value, catalog.revision)
        .await
        .unwrap();
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let models = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/models",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(models["models"].as_array().unwrap().len(), 1);
    assert_eq!(models["models"][0]["slug"], "gpt-6-astra");
    let response = app
        .clone()
        .oneshot(client_json(
            "POST",
            "/backend-api/codex/responses",
            &token,
            json!({"model":"gpt-5.6-luna","input":[]}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "model_not_entitled"
    );
    input["models"] = json!([{"provider_id":"chatgpt","model":"gpt-5.6-luna"}]);
    input["revision"] = plan.revision.into();
    assert_eq!(
        app.clone()
            .oneshot(admin_request(
                "PUT",
                &format!("/admin/api/plans/{}", plan.id),
                &cookie,
                &csrf,
                input
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let models = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/models",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(models["models"].as_array().unwrap().len(), 1);
    assert_eq!(models["models"][0]["slug"], "gpt-5.6-luna");
    let denied = app
        .clone()
        .oneshot(client_json(
            "POST",
            "/backend-api/codex/responses",
            &token,
            json!({"model":"gpt-6-astra","input":[]}),
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let identity = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/accounts/optimized/check",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(identity["account"]["plan_type"], "plus");
    assert_eq!(identity["account"]["id"], account.id);
    let config = storage
        .model_config("chatgpt", "gpt-5.6-luna")
        .await
        .unwrap()
        .unwrap();
    storage
        .set_model_enabled("chatgpt", &config.model, false, config.revision)
        .await
        .unwrap();
    let models = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/models",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(models["models"].as_array().unwrap().is_empty());
    let blocked = app
        .clone()
        .oneshot(client_json(
            "POST",
            "/backend-api/codex/responses",
            &token,
            json!({"model":"gpt-5.6-luna","input":[]}),
        ))
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(blocked).await["error"]["code"],
        "model_unavailable"
    );
    storage
        .set_model_enabled("chatgpt", &config.model, true, config.revision + 1)
        .await
        .unwrap();
    let models = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/models",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(models["models"].as_array().unwrap().len(), 1);
    storage
        .delete_model_config("chatgpt", &config.model, config.revision + 2)
        .await
        .unwrap();
    let blocked = app
        .clone()
        .oneshot(client_json(
            "POST",
            "/backend-api/codex/responses",
            &token,
            json!({"model":"gpt-5.6-luna","input":[]}),
        ))
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn desktop_support_records_actual_batches_and_authenticates_sdk_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("support.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    let saved = storage
        .virtual_config(&account.id, "feature_bootstrap")
        .await
        .unwrap();
    let mut gates = saved.value;
    let gate = codex2api_storage::statsig_hash("1867347216");
    gates["feature_gates"][&gate] = serde_json::json!({"value":true});
    codex2api_storage::prepare_client_config("feature_bootstrap", &mut gates);
    storage
        .update_virtual_admin_config(&account.id, "feature_bootstrap", &gates, saved.revision)
        .await
        .unwrap()
        .unwrap();
    let response = json_body(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/statsig/bootstrap",
                token,
                serde_json::json!({"stable_id":"sdk-device"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let bootstrap: Value =
        serde_json::from_str(response["statsigPayload"].as_str().unwrap()).unwrap();
    fn sdk(archive: &str, mode: &str, sample: Value) -> Value {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let mut child = Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/windows/Test-DesktopSupportContract.cjs"),
            )
            .arg(archive)
            .arg(mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sample.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode == "emit" {
            serde_json::from_slice(&output.stdout).unwrap()
        } else {
            Value::Null
        }
    }
    let default_init = serde_json::json!({"user":bootstrap["user"],"hash":"djb2","full_checksum":bootstrap["full_checksum"],"sinceTime":bootstrap["time"]});
    let requests = if let Ok(archive) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        sdk(&archive, "emit", serde_json::json!({"bootstrap":bootstrap}))
    } else {
        serde_json::json!([
            {"path":"/v1/initialize","body":base64::engine::general_purpose::STANDARD.encode(default_init.to_string()),"headers":{}},
            {"path":"/ces/v1/telemetry/intake","body":base64::engine::general_purpose::STANDARD.encode("{\"status\":\"info\",\"logger\":{\"name\":\"fixture\"},\"message\":\"PRIVATE_MESSAGE\"}"),"headers":{}},
            {"path":"/ces/v1/rgstr","body":base64::engine::general_purpose::STANDARD.encode("{\"events\":[{\"eventName\":\"fixture\",\"user\":{\"userID\":\"forged\"}}]}"),"headers":{}},
            {"path":"/v1/sdk_exception","body":base64::engine::general_purpose::STANDARD.encode("{\"tag\":\"fixture\",\"exception\":\"Error\",\"info\":\"PRIVATE_STACK\"}"),"headers":{}}
        ])
    };
    let make_request = |entry: &Value| {
        let path = entry["path"].as_str().unwrap();
        let mut request = Request::post(format!("{ROOT}{path}"));
        for (k, v) in entry["headers"].as_object().unwrap() {
            request = request.header(k, v.as_str().unwrap());
        }
        if path.starts_with("/ces/v1/rgstr") {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        request
            .body(Body::from(
                base64::engine::general_purpose::STANDARD
                    .decode(entry["body"].as_str().unwrap())
                    .unwrap(),
            ))
            .unwrap()
    };
    for entry in requests.as_array().unwrap() {
        for _ in 0..2 {
            let response = app.clone().oneshot(make_request(entry)).await.unwrap();
            if !response.status().is_success() {
                let raw = base64::engine::general_purpose::STANDARD
                    .decode(entry["body"].as_str().unwrap())
                    .unwrap();
                let sample = serde_json::from_slice::<Value>(&raw).unwrap_or_default();
                panic!(
                    "{} {} user={} header_names={:?}",
                    entry["path"],
                    response.status(),
                    sample["user"],
                    entry["headers"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .collect::<Vec<_>>()
                );
            }
        }
    }
    let logs = storage.desktop_diagnostics(i64::MAX).await.unwrap();
    assert!(logs.iter().any(|v| v["source"] == "telemetry"));
    assert!(logs.iter().any(|v| v["source"] == "sdk_exception"));
    assert!(
        logs.iter()
            .filter(|v| v["source"] == "statsig_events")
            .all(|v| v["owner"] == account.id)
    );
    assert!(
        logs.iter()
            .filter(|v| v["source"] != "statsig_events")
            .all(|v| v["owner"].is_null())
    );
    assert!(logs.iter().all(|v| v["attempts"].as_u64().unwrap() >= 2));
    assert!(!serde_json::to_string(&logs).unwrap().contains("PRIVATE_"));
    let config = storage
        .virtual_config(&account.id, "feature_bootstrap")
        .await
        .unwrap();
    let mut value = config.value;
    value["feature_gates"][&gate]["value"] = false.into();
    storage
        .update_virtual_admin_config(&account.id, "feature_bootstrap", &value, config.revision)
        .await
        .unwrap()
        .unwrap();
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    let request = |body: Value| {
        Request::post(format!("{ROOT}/v1/initialize"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let refreshed = json_body(
        app.clone()
            .oneshot(request(default_init.clone()))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(refreshed["feature_gates"][&gate]["value"], false);
    assert_eq!(refreshed["user"]["customIDs"]["account_id"], account.id);
    if let Ok(archive) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        sdk(
            &archive,
            "verify",
            serde_json::json!({"bootstrap":bootstrap,"refresh":refreshed}),
        );
    }
    let mut forged = default_init.clone();
    forged["user"]["customIDs"]["account_id"] = "other-account".into();
    assert_eq!(
        app.clone().oneshot(request(forged)).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let mut missing = default_init.clone();
    missing.as_object_mut().unwrap().remove("full_checksum");
    assert_eq!(
        app.clone()
            .oneshot(request(missing))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(default_init.to_string())
        .chars()
        .rev()
        .collect::<String>();
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("{ROOT}/v1/initialize?se=1"))
                .body(Body::from(encoded))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(
        &mut gzip,
        b"{\"events\":[{\"eventName\":\"compressed_event\"}]}",
    )
    .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(
                Request::post(format!("{ROOT}/ces/v1/rgstr?gz=1"))
                    .body(Body::from(gzip.finish().unwrap()))
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let device = storage
        .virtual_devices(&account.id)
        .await
        .unwrap()
        .remove(0);
    storage
        .revoke_virtual_device(&account.id, &device.id)
        .await
        .unwrap();
    assert_eq!(
        app.oneshot(request(default_init)).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn desktop_public_resources_preserve_the_shell_contract_and_client_preferences_stay_client_owned()
 {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("resources.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let shell =
        b"<!doctype html><script type=\"module\" src=\"/assets/main-BFDC70j-.js\"></script>";
    let headers = serde_json::json!({"content-type":"text/html","content-security-policy":"sandbox allow-scripts allow-same-origin; frame-ancestors app://-","permissions-policy":"camera=(self), microphone=(self)","access-control-allow-origin":"*"});
    storage
        .save_desktop_resource("/mcp-app.html", shell, &headers)
        .await
        .unwrap();
    storage
        .save_desktop_resource(
            "/assets/main-BFDC70j-.js",
            b"export const ready=true;",
            &serde_json::json!({"content-type":"text/javascript"}),
        )
        .await
        .unwrap();
    let manifest = serde_json::json!({"schemaVersion":1,"buildVersion":"26.915.4065.0","storeProductId":"9PLM9XGG6VKS","packageIdentity":"OpenAI.Codex"});
    storage
        .save_desktop_resource(
            "/codex-app-prod/windows-store-update.json",
            manifest.to_string().as_bytes(),
            &serde_json::json!({"content-type":"application/json"}),
        )
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("{ROOT}/mcp-app.html"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-security-policy"],
        headers["content-security-policy"].as_str().unwrap()
    );
    assert_eq!(
        response.headers()["permissions-policy"],
        headers["permissions-policy"].as_str().unwrap()
    );
    assert!(!response.headers().contains_key("set-cookie"));
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap().as_ref(),
        shell
    );
    for path in [
        "/assets/main-BFDC70j-.js",
        "/codex-app-prod/windows-store-update.json",
    ] {
        assert_eq!(
            app.clone()
                .oneshot(
                    Request::get(format!("{ROOT}{path}"))
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(
                Request::get(format!(
                    "{ROOT}/assets/arbitrary-file.js?url=http://evil.test"
                ))
                .body(Body::empty())
                .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(client_json(
            "PATCH",
            "/backend-api/settings/account_user_setting?feature=chat_theme&value=purple",
            token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let settings = storage
        .virtual_config(&account.id, "user_settings")
        .await
        .unwrap();
    assert_eq!(settings.value["settings"]["chat_theme"], "purple");
    assert_eq!(settings.write_origin, "client");
    let mut forbidden = settings.value.clone();
    forbidden["settings"]["chat_theme"] = "blue".into();
    assert!(
        storage
            .update_virtual_admin_config(
                &account.id,
                "user_settings",
                &forbidden,
                settings.revision
            )
            .await
            .is_err()
    );
    let mut policy = settings.value.clone();
    policy["flags"]["bazaar_consent_required"] = true.into();
    storage
        .update_virtual_admin_config(&account.id, "user_settings", &policy, settings.revision)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "PATCH",
                "/backend-api/settings/account_user_setting?feature=chat_theme&value=green",
                token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let settings = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/settings/user",
                token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(settings["settings"]["chat_theme"], "green");
    assert_eq!(settings["flags"]["bazaar_consent_required"], true);
    assert_eq!(app.clone().oneshot(client_json("POST","/backend-api/wham/onboarding/desktop/complete",token,serde_json::json!({"role":"engineering","conversational_onboarding_skipped":false,"onboarding_credit_reward_warning_shown":false}))).await.unwrap().status(),StatusCode::OK);
    let progress = storage
        .virtual_client_state(&account.id, "onboarding")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(progress.write_origin, "client");
    assert!(progress.updated_at_ms > 0);
}

#[tokio::test]
async fn desktop_wham_analytics_persist_owned_metadata_and_deduplicate() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wham-events.sqlite");
    let storage = Storage::open(&db).await.unwrap();
    let (app, account) = fixture(&storage).await;
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    let requests: Vec<Value> = if let Ok(archive) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        let output = std::process::Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/windows/Test-DesktopAnalyticsContract.cjs"),
            )
            .arg(archive)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    } else {
        vec![
            serde_json::json!({"path":"/wham/analytics-events/events","body":{"events":[{"event_type":"codex_action_event","event_params":{"thread_id":"thread-a","turn_id":"turn-a","action":"copy","metadata":"PRIVATE"}}]}}),
        ]
    };
    for request in &requests {
        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(client_json(
                    "POST",
                    &format!("/backend-api{}", request["path"].as_str().unwrap()),
                    &token,
                    request["body"].clone(),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert!(json_body(response).await.is_object());
        }
    }
    let activity = storage
        .virtual_analytics(&account.id, 0, i64::MAX)
        .await
        .unwrap();
    assert_eq!(activity.len(), requests.len());
    assert!(
        activity
            .iter()
            .any(|v| v["action"] == "copy" && v["turn_id"] == "turn-a")
    );
    assert!(
        !serde_json::to_string(&activity)
            .unwrap()
            .contains("PRIVATE")
    );
    let mut other = account.clone();
    other.id = "other".into();
    other.username = "other".into();
    storage.save_virtual_account(&other).await.unwrap();
    assert!(
        storage
            .virtual_analytics(&other.id, 0, i64::MAX)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/analytics-events/events",
                "invalid",
                requests[0]["body"].clone()
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/analytics-events/events",
                &token,
                serde_json::json!({"events":null})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let reopened = Storage::open(&db).await.unwrap();
    assert_eq!(
        reopened
            .virtual_analytics(&account.id, 0, i64::MAX)
            .await
            .unwrap(),
        activity
    );
}

#[tokio::test]
async fn exhausted_spending_limit_rejects_http_and_websocket_before_upstream() {
    use tokio_tungstenite::tungstenite::{Error, client::IntoClientRequest};
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("budget.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    for key in ["primary_cost_limit_usd", "weekly_cost_limit_usd"] {
        let mut config =
            serde_json::json!({"primary_cost_limit_usd":10,"weekly_cost_limit_usd":20});
        let revision = storage
            .virtual_config(&account.id, "quota")
            .await
            .unwrap()
            .revision;
        set_test_plan_config(&storage, &account.id, "quota", &config, revision)
            .await
            .unwrap();
        let allowed = json_body(
            app.clone()
                .oneshot(client_json(
                    "GET",
                    "/backend-api/wham/usage",
                    &token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        let quota = storage.virtual_config(&account.id, "quota").await.unwrap();
        config[key] = 0.into();
        set_test_plan_config(&storage, &account.id, "quota", &config, quota.revision)
            .await
            .unwrap();
        let exhausted = json_body(
            app.clone()
                .oneshot(client_json(
                    "GET",
                    "/backend-api/wham/usage",
                    &token,
                    Value::Null,
                ))
                .await
                .unwrap(),
        )
        .await;
        if let Ok(archive) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
            use std::io::Write;
            let mut child = std::process::Command::new("node")
                .arg(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../scripts/windows/Test-DesktopQuotaContract.cjs"),
                )
                .arg(archive)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            write!(
                child.stdin.take().unwrap(),
                "{}",
                serde_json::json!({"allowed":allowed,"exhausted":exhausted})
            )
            .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if std::env::var_os("CODEX2API_TEST_CLI").is_some() {
            use std::io::Write;
            let mut child = std::process::Command::new("python")
                .arg(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../scripts/windows/Test-DesktopRateLimits.py"),
                )
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            write!(child.stdin.take().unwrap(), "{exhausted}").unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
        let http = app
            .clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/codex/responses",
                &token,
                serde_json::json!({"model":"gpt-6-astra","input":[]}),
            ))
            .await
            .unwrap();
        assert_eq!(http.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            json_body(http).await["error"]["code"],
            "virtual_quota_exceeded"
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve_app = app.clone();
        let server = tokio::spawn(async move { axum::serve(listener, serve_app).await.unwrap() });
        let mut request = format!("ws://{addr}{ROOT}/backend-api/codex/responses")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        match tokio_tungstenite::connect_async(request).await {
            Err(Error::Http(response)) => {
                assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS)
            }
            other => panic!("expected 429 before upstream, got {other:?}"),
        }
        server.abort();
    }
}

#[tokio::test]
async fn installed_plugins_generate_desktop_pagination_for_legacy_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("desktop-plugins.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    seed_captured_config(&storage,&account.id,"installed_plugins",&serde_json::json!({"plugins":[],"nextPageToken":"stale-token","pagination":{"next_page_token":"stale-token"}}),None).await.unwrap();
    let path = "/backend-api/ps/plugins/installed?scope=GLOBAL&limit=200";
    let response = app
        .clone()
        .oneshot(client_json("GET", path, &token, Value::Null))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let value = json_body(response).await;
    assert_eq!(
        value,
        serde_json::json!({"plugins":[],"pagination":{"limit":200,"next_page_token":null}})
    );
    if std::env::var("CODEX2API_TEST_CLI").is_ok() {
        use std::io::Write;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/windows/Test-DesktopInstalledPlugins.py");
        let mut child = std::process::Command::new("python")
            .arg(script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(), "{value}").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
    seed_captured_config(
        &storage,
        &account.id,
        "installed_plugins",
        &serde_json::json!({"plugins":[{"id":"own-a"},{"id":"own-b"}]}),
        None,
    )
    .await
    .unwrap();
    let first = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/ps/plugins/installed?limit=1",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(first["plugins"][0]["id"], "own-a");
    assert_eq!(first["pagination"]["next_page_token"], "1");
    let last = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/ps/plugins/installed?limit=1&pageToken=1",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(last["plugins"][0]["id"], "own-b");
    assert!(last["pagination"]["next_page_token"].is_null());
    for query in ["limit=0", "pageToken=bad", "limit=1&limit=2"] {
        assert_eq!(
            app.clone()
                .oneshot(client_json(
                    "GET",
                    &format!("/backend-api/ps/plugins/installed?{query}"),
                    &token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn desktop_bootstrap_carries_the_execution_settings_identity() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("bootstrap-identity.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    let saved = storage
        .virtual_config(&account.id, "feature_bootstrap")
        .await
        .unwrap();
    storage.update_virtual_config(&account.id,"feature_bootstrap",&serde_json::json!({"feature_gates":{"fixture_gate":{"name":"fixture_gate","value":true,"rule_id":"local"}},"dynamic_configs":{},"layer_configs":{},"has_updates":false,"user":{"userID":"stored-foreign-user"}}),saved.revision).await.unwrap().unwrap();
    let response=app.clone().oneshot(client_json("POST","/backend-api/wham/statsig/bootstrap",&token,serde_json::json!({"stable_id":"desktop-installation-A","user":{"userID":"forged-user","customIDs":{"account_id":"forged-account"}}}))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = json_body(response).await;
    let payload: Value =
        serde_json::from_str(response["statsigPayload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["user"]["customIDs"]["account_id"], account.id);
    assert_eq!(payload["user"]["userID"], format!("user-{}", account.id));
    assert_eq!(payload["user"]["custom"]["auth_method"], "chatgpt");
    assert_eq!(
        payload["user"]["customIDs"]["stableID"],
        "desktop-installation-A"
    );
    assert_eq!(
        payload["user"]["customIDs"],
        payload["evaluated_keys"]["customIDs"]
    );
    assert_eq!(payload["has_updates"], true);
    assert_eq!(payload["user"]["custom"]["desktop_app_beta_enabled"], false);
    assert_eq!(payload["feature_gates"]["fixture_gate"]["value"], true);
    assert!(!payload.to_string().contains("stored-foreign-user"));
    let beta = json_body(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/statsig/bootstrap",
                &token,
                serde_json::json!({"desktop_app_beta_enabled":true}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let beta: Value = serde_json::from_str(beta["statsigPayload"].as_str().unwrap()).unwrap();
    assert_eq!(beta["user"]["custom"]["desktop_app_beta_enabled"], true);
    assert!(!payload.to_string().contains("forged"));
    let other = json_body(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/statsig/bootstrap",
                &token,
                serde_json::json!({"stable_id":"desktop-installation-B"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let other: Value = serde_json::from_str(other["statsigPayload"].as_str().unwrap()).unwrap();
    assert_eq!(
        other["user"]["customIDs"]["stableID"],
        "desktop-installation-B"
    );
    assert_eq!(other["user"]["customIDs"]["account_id"], account.id);
    for invalid in [serde_json::json!({"stable_id":{}}), Value::Null] {
        assert_eq!(
            app.clone()
                .oneshot(client_json(
                    "POST",
                    "/backend-api/wham/statsig/bootstrap",
                    &token,
                    invalid
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    if let Ok(archive) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        use std::io::Write;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/windows/Test-DesktopBootstrapContract.cjs");
        let mut child = std::process::Command::new("node")
            .arg(script)
            .arg(archive)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        write!(child.stdin.take().unwrap(),"{}",serde_json::json!({"response":response,"accountId":account.id,"userId":format!("user-{}",account.id),"stableId":"desktop-installation-A"})).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
    let preferences = storage
        .virtual_config(&account.id, "desktop_preferences")
        .await
        .unwrap();
    storage
        .update_virtual_config(
            &account.id,
            "desktop_preferences",
            &serde_json::json!({"localized_interface":false,"profile_enabled":false}),
            preferences.revision,
        )
        .await
        .unwrap()
        .unwrap();
    let disabled = json_body(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/statsig/bootstrap",
                &token,
                serde_json::json!({}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let disabled: Value =
        serde_json::from_str(disabled["statsigPayload"].as_str().unwrap()).unwrap();
    assert_eq!(
        disabled["layer_configs"][codex2api_storage::statsig_hash("72216192")]["value"]["enable_i18n"],
        true
    );
    assert_eq!(
        disabled["feature_gates"][codex2api_storage::statsig_hash("2478676115")]["value"],
        true
    );
}

#[tokio::test]
async fn referral_tracking_filters_the_virtual_records_and_paginates_without_looping() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("referrals.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = "/backend-api/referrals/invite/tracking?program_id=codex_referral_consumer&limit=1&period=past_90_days";
    assert_eq!(
        app.clone()
            .oneshot(
                Request::get(format!("{ROOT}{path}"))
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let empty = json_body(
        app.clone()
            .oneshot(client_json("GET", path, &token, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(empty, serde_json::json!({"items":[],"cursor":null}));
    let now = chrono::Utc::now();
    let item = |id: &str, days: i64, program: &str| serde_json::json!({"referral_id":id,"email":format!("{id}@example.test"),"program_id":program,"created_at":(now-chrono::Duration::days(days)).to_rfc3339(),"status":"pending"});
    let config = storage
        .virtual_config(&account.id, "referral_tracking")
        .await
        .unwrap();
    storage.update_virtual_config(&account.id,"referral_tracking",&serde_json::json!({"items":[item("recent",0,"codex_referral_consumer"),item("older",2,"codex_referral_consumer"),item("expired",91,"codex_referral_consumer"),item("workspace",0,"codex_referral_workspace")]}),config.revision).await.unwrap().unwrap();
    let first = json_body(
        app.clone()
            .oneshot(client_json("GET", path, &token, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(first["items"][0]["referral_id"], "recent");
    assert_eq!(first["cursor"], "1");
    assert_eq!(first["items"][0]["can_resend"], false);
    let second = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                &format!("{path}&cursor=1"),
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(second["items"][0]["referral_id"], "older");
    assert!(second["cursor"].is_null());
    let month=json_body(app.clone().oneshot(client_json("GET","/backend-api/referrals/invite/tracking?program_id=codex_referral_consumer&period=this_month",&token,Value::Null)).await.unwrap()).await;
    assert_eq!(month["items"][0]["referral_id"], "recent");
    assert_eq!(
        month["items"].as_array().unwrap().len(),
        if (now - chrono::Duration::days(2))
            .format("%Y-%m")
            .to_string()
            == now.format("%Y-%m").to_string()
        {
            2
        } else {
            1
        }
    );
    let ended = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                &format!("{path}&cursor={}", usize::MAX),
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(ended["items"], serde_json::json!([]));
    assert!(ended["cursor"].is_null());
    if let Ok(archive) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        use std::io::Write;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/windows/Test-DesktopReferralContract.cjs");
        let mut child = std::process::Command::new("node")
            .arg(script)
            .arg(archive)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        write!(
            child.stdin.take().unwrap(),
            "{}",
            serde_json::json!({"first":first,"second":second})
        )
        .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
    let mut other = account.clone();
    other.id = uuid::Uuid::new_v4().to_string();
    other.username = "referral-other".into();
    storage.save_virtual_account(&other).await.unwrap();
    assert_eq!(
        storage
            .virtual_config(&other.id, "referral_tracking")
            .await
            .unwrap()
            .value["items"],
        serde_json::json!([])
    );
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "GET",
                &format!("{path}&account_id={}", other.id),
                &token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    for query in [
        "program_id=p&period=invalid",
        "program_id=p&limit=0",
        "program_id=p&cursor=bad",
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json(
                    "GET",
                    &format!("/backend-api/referrals/invite/tracking?{query}"),
                    &token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert!(storage.missing_endpoints().await.unwrap().is_empty());
}

#[tokio::test]
async fn desktop_services_authenticate_and_heartbeat_updates_virtual_device_without_supplier() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("desktop-services.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    for (method, path) in [
        (
            "GET",
            "/backend-api/connectors/directory/list?external_logos=true",
        ),
        (
            "GET",
            "/backend-api/aura/site_status?site_url=https%3A%2F%2Fexample.test",
        ),
        ("POST", "/backend-api/sentinel/heartbeat"),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(format!("{ROOT}{path}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let response = app
            .clone()
            .oneshot(client_json(method, path, &token, Value::Null))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if path.ends_with("heartbeat") {
                StatusCode::NO_CONTENT
            } else if path.contains("connectors") {
                StatusCode::NOT_IMPLEMENTED
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        );
    }
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("{ROOT}/backend-api/sentinel/heartbeat"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        storage.virtual_devices(&account.id).await.unwrap()[0]
            .last_used_at
            .is_some()
    );
    assert!(
        storage
            .virtual_request_logs(&account.id, i64::MAX)
            .await
            .unwrap()
            .iter()
            .any(|v| v["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("/sentinel/heartbeat"))
                && v["status"] == 204)
    );
    assert!(storage.missing_endpoints().await.unwrap().is_empty());
}

#[tokio::test]
async fn virtual_management_reads_persisted_private_data_and_isolates_actual_activity() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("managed.sqlite");
    let storage = Storage::open(&db).await.unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    let mut other = account.clone();
    other.id = uuid::Uuid::new_v4().to_string();
    other.username = "bob".into();
    storage.save_virtual_account(&other).await.unwrap();
    let supplier_id = test_supplier(&storage, &account).await.unwrap();
    let source = supplier_id.as_str();
    let conduit = storage
        .create_virtual_conduit(&account.id, source, "private-upstream-conduit")
        .await
        .unwrap();
    assert!(
        storage
            .virtual_conduit(&other.id, source, &conduit)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .virtual_conduit(&account.id, "different-source", &conduit)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .virtual_conduit(&account.id, source, &conduit)
            .await
            .unwrap()
            .as_deref(),
        Some("private-upstream-conduit")
    );
    for (key, path, value) in [
        (
            "payment_methods",
            "/backend-api/payments/payment_methods",
            serde_json::json!({"payment_methods":[{"id":"virtual-card","last4":"1234"}]}),
        ),
        (
            "pins",
            "/backend-api/pins",
            serde_json::json!([{"id":"pin-a","item_type":"conversation"}]),
        ),
        (
            "first_party",
            "/backend-api/aip/first-party/eligibility",
            serde_json::json!({"finances":true,"health_eligibility":{"sidebar_visible":true}}),
        ),
        (
            "system_hints",
            "/backend-api/system_hints",
            serde_json::json!({"system_hints":[{"id":"own-hint","title":"Own hint","name":"Own hint","description":"Own description","system_hint":"search","aliases":[],"required_features":[],"required_models":[],"required_conversation_modes":[],"allow_in_temporary_chat":true}]}),
        ),
    ] {
        let saved = storage.virtual_config(&account.id, key).await.unwrap();
        assert!(
            storage
                .update_virtual_config(&account.id, key, &value, saved.revision)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            storage
                .update_virtual_config(&account.id, key, &value, saved.revision)
                .await
                .unwrap()
                .is_none()
        );
        let response = app
            .clone()
            .oneshot(client_json("GET", path, token, Value::Null))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await, value);
        assert_ne!(
            storage.virtual_config(&other.id, key).await.unwrap().value,
            value
        );
    }
    for path in [
        format!("/backend-api/accounts/{}/settings", other.id),
        format!(
            "/backend-api/accounts/{}/spend-controls/current-user/monthly-usage",
            other.id
        ),
        format!(
            "/backend-api/payments/payment_methods?account_id={}&account_id={}",
            account.id, other.id
        ),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json("GET", &path, token, Value::Null))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    let events = serde_json::json!({"events":[
        {"event_type":"codex_turn_event","event_params":{"thread_id":"thread-a","turn_id":"turn-a","model":"fixture-model","app_server_client":{"product_client_id":"CODEX_CLI"},"prompt":"must-not-be-stored"}},
        {"event_type":"skill_invocation","skill_id":"skill-a","skill_name":"My Skill","event_params":{"thread_id":"thread-a","turn_id":"turn-a"}},
        {"event_type":"codex_plugin_used","event_params":{"plugin_id":"plugin-a","plugin_name":"My Plugin","thread_id":"thread-a","turn_id":"turn-a"}}
    ]});
    // Retrying the same telemetry batch must not double count events.
    for _ in 0..2 {
        assert_eq!(
            app.clone()
                .oneshot(client_json(
                    "POST",
                    "/backend-api/codex/analytics-events/events",
                    token,
                    events.clone()
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    let metrics = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/analytics/daily-workspace-usage-counts",
                token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(metrics["data"][0]["totals"]["turns"], 1);
    let plugins = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/analytics/daily-plugin-usage-metrics",
                token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        plugins["data"][0]["plugin_usage_overviews"][0]["invocation_counts"],
        1
    );
    let persisted = storage
        .virtual_analytics(&account.id, 0, i64::MAX)
        .await
        .unwrap();
    assert_eq!(persisted.len(), 3);
    assert!(
        !serde_json::to_string(&persisted)
            .unwrap()
            .contains("must-not-be-stored")
    );
    assert!(
        storage
            .virtual_analytics(&other.id, 0, i64::MAX)
            .await
            .unwrap()
            .is_empty()
    );
    let quota = storage.virtual_config(&account.id, "quota").await.unwrap();
    set_test_plan_config(
        &storage,
        &account.id,
        "quota",
        &serde_json::json!({"primary_cost_limit_usd":null,"weekly_cost_limit_usd":100}),
        quota.revision,
    )
    .await
    .unwrap();
    record_usage(
        &storage,
        &account,
        test_supplier(&storage, &account).await.as_deref().unwrap(),
        "actual-owned-usage",
        49,
    )
    .await;
    let usage = storage.virtual_quota(&account.id).await.unwrap();
    assert_eq!(usage["rate_limit"]["primary_window"]["limit_usd"], "100");
    assert_eq!(
        usage["rate_limit"]["primary_window"]["limit_window_seconds"],
        604800
    );
    assert!(usage["rate_limit"]["secondary_window"].is_null());
    assert_eq!(usage["billing"]["unpriced_requests"], 1);
    storage
        .save_virtual_resource(
            &other.id,
            "task",
            "foreign-task",
            test_supplier(&storage, &other).await.as_deref(),
            &serde_json::json!({"task":{"id":"foreign-task","title":"PRIVATE"}}),
        )
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/tasks/foreign-task",
                token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let tasks = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/tasks/list",
                token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(tasks["items"], serde_json::json!([]));
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    let reopened = Storage::open(&db).await.unwrap();
    assert_eq!(
        reopened
            .virtual_config(&account.id, "payment_methods")
            .await
            .unwrap()
            .value["payment_methods"][0]["last4"],
        "1234"
    );
    assert_eq!(
        reopened.virtual_quota(&account.id).await.unwrap()["rate_limit"]["primary_window"]["limit_usd"],
        "100"
    );
    assert_eq!(
        reopened
            .virtual_usage_summary(&account.id)
            .await
            .unwrap()
            .lifetime_tokens,
        Some(50)
    );
    let logs = reopened
        .virtual_request_logs(&account.id, i64::MAX)
        .await
        .unwrap();
    assert!(!logs.is_empty());
    let log_text = serde_json::to_string(&logs).unwrap();
    assert!(!log_text.contains(token));
    assert!(!log_text.contains("account_id="));
    assert!(
        reopened
            .virtual_request_logs(&other.id, i64::MAX)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(reopened.missing_endpoints().await.unwrap().is_empty());
}

#[tokio::test]
async fn virtual_pubsub_tickets_deliver_only_owned_events_and_revoke_with_device() {
    use futures::{SinkExt, StreamExt};
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("pubsub.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server_app = app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, server_app).await.unwrap() });
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("{ROOT}/backend-api/celsius/ws/user"))
                .header("host", address.to_string())
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let value = json_body(response).await;
    let url = value["websocket_url"].as_str().unwrap();
    assert!(!url.contains(token));
    assert!(
        tokio_tungstenite::connect_async(format!("{url}tampered"))
            .await
            .is_err()
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        r#"[{"id":1,"command":{"type":"subscribe","topic_id":"app_notifications"}}]"#.into(),
    ))
    .await
    .unwrap();
    let reply = tokio::time::timeout(std::time::Duration::from_secs(3), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(reply.to_text().unwrap()).unwrap()[0]["reply"]["type"],
        "subscribe"
    );
    let saved = storage
        .virtual_config(&account.id, "notifications")
        .await
        .unwrap();
    storage.update_virtual_config(&account.id,"notifications",&serde_json::json!({"items":[{"id":"notify-a","notification_type":"setting-updated","payload":{"clear_cache_only":true}}],"cursor":null}),saved.revision).await.unwrap();
    let event = tokio::time::timeout(std::time::Duration::from_secs(4), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(event.to_text().unwrap()).unwrap()[0]["payload"]["notification_id"],
        "notify-a"
    );
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        r#"[{"id":2,"command":{"type":"subscribe","topic_id":"conversations"}}]"#.into(),
    ))
    .await
    .unwrap();
    ws.next().await.unwrap().unwrap();
    let mut other = account.clone();
    other.id = "pubsub-other".into();
    other.username = "pubsub-other".into();
    storage.save_virtual_account(&other).await.unwrap();
    storage
        .save_virtual_resource(
            &other.id,
            "conversation",
            "foreign-conversation",
            None,
            &serde_json::json!({"id":"foreign-conversation"}),
        )
        .await
        .unwrap();
    storage
        .save_virtual_resource(
            &account.id,
            "conversation",
            "actual-conversation",
            None,
            &serde_json::json!({"id":"actual-conversation","status":"in_progress"}),
        )
        .await
        .unwrap();
    let event = tokio::time::timeout(std::time::Duration::from_secs(4), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let event: Value = serde_json::from_str(event.to_text().unwrap()).unwrap();
    assert_eq!(event[0]["payload"]["type"], "conversation-created");
    assert!(!event.to_string().contains("foreign-conversation"));
    storage
        .save_virtual_resource(
            &account.id,
            "conversation",
            "actual-conversation",
            None,
            &serde_json::json!({"id":"actual-conversation","status":"finished_successfully"}),
        )
        .await
        .unwrap();
    let event = tokio::time::timeout(std::time::Duration::from_secs(4), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let event: Value = serde_json::from_str(event.to_text().unwrap()).unwrap();
    assert!(
        event
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["payload"]["type"] == "conversation-turn-complete")
    );
    let device = storage
        .virtual_devices(&account.id)
        .await
        .unwrap()
        .remove(0);
    storage
        .revoke_virtual_device(&account.id, &device.id)
        .await
        .unwrap();
    assert!(tokio_tungstenite::connect_async(url).await.is_err());
    let closed = tokio::time::timeout(std::time::Duration::from_secs(4), ws.next())
        .await
        .unwrap();
    assert!(
        closed.is_none()
            || matches!(
                closed,
                Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))
            )
    );
    server.abort();
}

fn client_json(method: &str, path: &str, token: &str, value: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(format!("{ROOT}{path}"))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}

#[tokio::test]
async fn desktop_usage_reads_actual_virtual_tokens_across_bindings() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("usage.sqlite");
    let storage = Storage::open(&db).await.unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    let routes = [
        "/backend-api/wham/analytics/daily-workspace-usage-counts",
        "/backend-api/wham/usage/daily-token-usage-breakdown",
        "/backend-api/wham/usage/credit-usage-events",
        "/backend-api/wham/analytics/daily-plugin-usage-metrics",
        "/backend-api/wham/usage/plan_limit_history",
        "/backend-api/wham/analytics/daily-skill-usage-metrics",
    ];
    for path in routes {
        assert_eq!(
            app.clone()
                .oneshot(
                    Request::get(format!("{ROOT}{path}"))
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let response = app
            .clone()
            .oneshot(client_json("GET", path, token, Value::Null))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let value = json_body(response).await;
        if path.ends_with("plan_limit_history") {
            assert_eq!(value["periods"], serde_json::json!([]));
            assert_eq!(value["coverage_complete"], false);
            assert!(value["data_as_of"].is_null());
        } else {
            assert_eq!(value["data"], serde_json::json!([]));
        }
    }
    let supplier = test_supplier(&storage, &account).await.clone().unwrap();
    for (id, source, bound, date, input, output, cached, actual) in [
        (
            "one",
            account.id.as_str(),
            supplier.as_str(),
            "2026-09-12T00:00:00Z",
            Some(100),
            Some(20),
            Some(80),
            Some("actual-model"),
        ),
        (
            "two",
            account.id.as_str(),
            "second-supplier",
            "2026-09-18T23:59:59Z",
            Some(50),
            Some(10),
            None,
            Some("actual-model"),
        ),
        (
            "three",
            account.id.as_str(),
            supplier.as_str(),
            "2026-09-18T12:00:00Z",
            None,
            Some(7),
            None,
            None,
        ),
        (
            "unknown",
            account.id.as_str(),
            supplier.as_str(),
            "2026-09-18T12:00:00Z",
            None,
            None,
            None,
            None,
        ),
        (
            "foreign",
            "different-source",
            supplier.as_str(),
            "2026-09-18T12:00:00Z",
            Some(9000),
            Some(9000),
            None,
            None,
        ),
        (
            "before",
            account.id.as_str(),
            supplier.as_str(),
            "2026-09-11T23:59:59Z",
            Some(9000),
            None,
            None,
            None,
        ),
        (
            "after",
            account.id.as_str(),
            supplier.as_str(),
            "2026-09-19T00:00:00Z",
            Some(9000),
            None,
            None,
            None,
        ),
    ] {
        let mut row = UsageRecord {
            id: id.into(),
            subject_id: source.into(),
            account_id: bound.into(),
            model: Some("requested-model".into()),
            actual_model: actual.map(str::to_owned),
            requested_at_ms: chrono::DateTime::parse_from_rfc3339(date)
                .unwrap()
                .timestamp_millis(),
            status: "in_progress".into(),
            ..Default::default()
        };
        storage.insert_usage(&row).await.unwrap();
        row.input_tokens = input;
        row.output_tokens = output;
        row.cached_tokens = cached;
        row.status = "completed".into();
        storage.finish_usage(&row).await.unwrap();
    }
    let path = format!(
        "{}?start_date=2026-09-12&end_date=2026-09-18&group_by=day",
        routes[1]
    );
    let value = json_body(
        app.clone()
            .oneshot(client_json("GET", &path, token, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(value["units"], "tokens");
    assert_eq!(
        value["data"],
        serde_json::json!([
            {"date":"2026-09-12","models":[{"model":"actual-model","credits":120}],"product_surface_usage_values":{"codex":120}},
            {"date":"2026-09-18","models":[{"model":"actual-model","credits":60},{"model":"requested-model","credits":7}],"product_surface_usage_values":{"codex":67}}
        ])
    );
    if let Ok(desktop) = std::env::var("CODEX2API_TEST_DESKTOP_EXE") {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut samples = serde_json::Map::new();
        for (key, path) in ["counts", "tokens", "credits", "plugins", "plan", "skills"]
            .into_iter()
            .zip(routes)
        {
            let path = format!("{path}?start_date=2026-09-12&end_date=2026-09-18&group_by=day");
            samples.insert(
                key.into(),
                json_body(
                    app.clone()
                        .oneshot(client_json("GET", &path, token, Value::Null))
                        .await
                        .unwrap(),
                )
                .await,
            );
        }
        let archive = std::path::Path::new(&desktop)
            .parent()
            .unwrap()
            .join("resources/app.asar");
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/windows/Test-DesktopUsage.cjs");
        let mut child = Command::new("node")
            .arg(script)
            .arg(archive)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(Value::Object(samples).to_string().as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        println!("{}", String::from_utf8_lossy(&result.stdout));
    }
    // Historical analytics remain virtual-account data after unbinding.
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    let reopened = Storage::open(&db).await.unwrap();
    assert_eq!(
        json_body(
            router(&reopened)
                .oneshot(client_json("GET", &path, token, Value::Null))
                .await
                .unwrap()
        )
        .await,
        value
    );
    for query in [
        "start_date=bad",
        "start_date=2026-09-19&end_date=2026-09-18",
        "group_by=month",
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json(
                    "GET",
                    &format!("{}?{query}", routes[1]),
                    token,
                    Value::Null
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    // These reads create neither inference ledger entries nor missing-endpoint records.
    assert_eq!(
        storage
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        7
    );
    assert!(storage.missing_endpoints().await.unwrap().is_empty());
}

#[tokio::test]
async fn desktop_cloud_reads_are_isolated_and_client_settings_survive_rebinding_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("client-state.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    for path in [
        "/backend-api/wham/sites/access",
        "/backend-api/wham/onboarding/context",
        "/backend-api/automations",
        "/backend-api/conversations",
        "/backend-api/beacons/home",
        "/backend-api/accounts/verified_access",
        "/backend-api/wham/browser/settings",
        "/backend-api/wham/analytics/daily-code-review-metrics",
    ] {
        let denied = app
            .clone()
            .oneshot(
                Request::get(format!("{ROOT}{path}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(client_json("GET", path, token, Value::Null))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let value = json_body(response).await;
        assert!(!value.to_string().contains("real-"));
        if path.ends_with("/automations") || path.ends_with("/conversations") {
            assert_eq!(value["items"], serde_json::json!([]));
        }
        if path.ends_with("verified_access") {
            assert_eq!(value["programs"], serde_json::json!([]));
        }
        if path.ends_with("browser/settings") {
            assert_eq!(value["preferences"]["approval_mode"], "always_ask");
            assert_eq!(value["revision"], 0);
        }
        if path.ends_with("daily-code-review-metrics") {
            assert_eq!(value["data"], serde_json::json!([]));
        }
    }
    let browser = "/backend-api/wham/browser/settings";
    let patch = serde_json::json!({"expected_revision":0,"preferences":{"history_approval_mode":"disabled","webmcp_enabled":false},"rule_updates":[{"resource":"origin","pattern":"https://fixture.example","decision":"deny"}]});
    let saved = app
        .clone()
        .oneshot(client_json("PATCH", browser, token, patch.clone()))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    assert_eq!(json_body(saved).await["revision"], 1);
    assert_eq!(
        app.clone()
            .oneshot(client_json("PATCH", browser, token, patch))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let invalid =
        serde_json::json!({"expected_revision":1,"preferences":{"approval_mode":"unsupported"}});
    assert_eq!(
        app.clone()
            .oneshot(client_json("PATCH", browser, token, invalid))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let update = || {
        client_json(
            "PATCH",
            browser,
            token,
            serde_json::json!({"expected_revision":1,"preferences":{"disable_auto_review":true}}),
        )
    };
    let (first, second) =
        tokio::join!(app.clone().oneshot(update()), app.clone().oneshot(update()));
    let mut statuses = [
        first.unwrap().status().as_u16(),
        second.unwrap().status().as_u16(),
    ];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    let completed=app.clone().oneshot(client_json("POST","/backend-api/wham/onboarding/desktop/complete",token,serde_json::json!({"role":"coding","conversational_onboarding_skipped":false,"onboarding_credit_reward_warning_shown":true}))).await.unwrap();
    assert_eq!(completed.status(), StatusCode::OK);
    let selected = app
        .clone()
        .oneshot(client_json(
            "PATCH",
            "/backend-api/settings/account_user_setting?feature=voice_name&value=cedar",
            token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(selected.status(), StatusCode::OK);
    let other_id = uuid::Uuid::new_v4().to_string();
    let mut other = account.clone();
    other.id = other_id.clone();
    other.username = "second".into();
    storage.save_virtual_account(&other).await.unwrap();
    assert!(
        storage
            .virtual_client_state(&other_id, "browser_settings")
            .await
            .unwrap()
            .is_none()
    );
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    for (method, path) in [
        (
            "GET",
            "/backend-api/settings/voices?spoken_language=en-US&voice_mode=advanced",
        ),
        ("POST", "/backend-api/o11y/v1/traces"),
    ] {
        let response = app
            .clone()
            .oneshot(client_json(method, path, token, serde_json::json!({})))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if path.ends_with("/traces") {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            },
            "{path}"
        );
    }
    assert!(storage.missing_endpoints().await.unwrap().is_empty());
    assert_eq!(
        storage
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        0
    );
    drop(app);
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    let persisted = storage
        .virtual_client_state(&account.id, "browser_settings")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.revision, 2);
    assert_eq!(
        persisted.value["preferences"]["history_approval_mode"],
        "disabled"
    );
    assert_eq!(
        persisted.value["rules"]["origin"]["https://fixture.example"],
        "deny"
    );
    let onboarding = storage
        .virtual_client_state(&account.id, "onboarding")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(onboarding.value["role"], "coding");
    assert!(onboarding.value["desktop_onboarding_completed_at"].is_string());
    assert_eq!(
        storage
            .virtual_client_state(&account.id, "voice")
            .await
            .unwrap()
            .unwrap()
            .value["selected"],
        "cedar"
    );
    assert!(
        storage
            .save_virtual_client_state(
                &other_id,
                "browser_settings",
                &serde_json::json!({}),
                Some(5)
            )
            .await
            .unwrap()
            .is_none()
    );
    storage.delete_virtual_account(&account.id).await.unwrap();
    assert!(
        storage
            .virtual_client_state(&account.id, "browser_settings")
            .await
            .unwrap()
            .is_none()
    );
    storage.close().await;
}

#[tokio::test]
async fn desktop_account_settings_and_subscription_use_virtual_identity() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("desktop-profile.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    for path in [
        "/backend-api/accounts/optimized/check",
        "/backend-api/accounts/check/v4-2023-04-27",
        "/backend-api/me",
        "/backend-api/settings/user",
        "/backend-api/subscriptions",
        "/backend-api/subscriptions/auto_top_up/settings",
        "/backend-api/subscriptions/credits/discount-offer",
        "/backend-api/wham/accounts/check",
        "/backend-api/wham/usage",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("{ROOT}{path}"))
                    .header(
                        "authorization",
                        format!("Bearer {}", tokens["access_token"].as_str().unwrap()),
                    )
                    .header("chatgpt-account-id", &account.id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let value = json_body(response).await;
        assert!(!value.to_string().contains("real-"));
        if path == "/backend-api/accounts/check/v4-2023-04-27" {
            assert_eq!(
                value["accounts"][&account.id]["account"]["plan_type"],
                "pro"
            );
            assert_eq!(
                value["accounts"][&account.id]["account"]["account_user_id"],
                format!("user-{}", account.id)
            );
        }
        if path == "/backend-api/accounts/optimized/check" {
            assert!(value.get("accounts").is_none());
            assert_eq!(value["account"]["account_id"], account.id);
            assert_eq!(value["account"]["plan_type"], "pro");
            assert_eq!(value["account_user"]["account_id"], account.id);
            assert_eq!(
                value["account_user"]["user_id"],
                format!("user-{}", account.id)
            );
            assert!(
                value["account_user"]
                    .as_object()
                    .unwrap()
                    .contains_key("seat_type")
            );
            assert!(value["account_user"]["seat_type"].is_null());
            assert_eq!(value["account_user"]["pending_seat_upgrade_request"], false);
        }
        if path.ends_with("/me") {
            assert_eq!(value["name"], account.name);
            assert_eq!(value["email"], account.email);
        }
        if path.ends_with("/subscriptions") {
            assert_eq!(
                value["active_until"],
                account.subscription_expires_at.clone().unwrap()
            );
        }
        if path.contains("auto_top_up") {
            assert_eq!(value["is_enabled"], false);
            assert!(value["payment_method"].is_null());
        }
        if path == "/backend-api/wham/accounts/check" {
            assert_eq!(value["accounts"][0]["id"], account.id);
        }
    }
    storage.close().await;
}

#[tokio::test]
async fn mcp_discovery_is_public_and_batch_and_mcp_require_virtual_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("mcp-discovery.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let metadata = format!("{ROOT}/backend-api/ps/mcp/.well-known/oauth-protected-resource");
    let response = app
        .clone()
        .oneshot(
            Request::get(&metadata)
                .header("host", "127.0.0.1:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let value = json_body(response).await;
    assert_eq!(
        value["resource"],
        format!("http://127.0.0.1:8080{ROOT}/backend-api/ps/mcp")
    );
    assert_eq!(
        value["bearer_methods_supported"],
        serde_json::json!(["header"])
    );
    assert!(!value.to_string().contains("auth.openai.com"));
    let tokens = login(&app).await;
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    for (method, path) in [
        ("POST", "/backend-api/ps/apps/batch"),
        ("POST", "/backend-api/codex/analytics-events/events"),
        ("GET", "/backend-api/ps/mcp"),
        ("POST", "/backend-api/ps/mcp"),
    ] {
        for authenticated in [false, true] {
            let mut request = Request::builder()
                .method(method)
                .uri(format!("{ROOT}{path}"));
            if authenticated {
                request = request.header(
                    "authorization",
                    format!("Bearer {}", tokens["access_token"].as_str().unwrap()),
                );
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                if authenticated && path.ends_with("/analytics-events/events") {
                    StatusCode::BAD_REQUEST
                } else if authenticated && path.ends_with("/ps/mcp") {
                    if method == "POST" {
                        StatusCode::BAD_REQUEST
                    } else {
                        StatusCode::NOT_IMPLEMENTED
                    }
                } else if authenticated {
                    StatusCode::NOT_IMPLEMENTED
                } else {
                    StatusCode::UNAUTHORIZED
                }
            );
        }
    }
    let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
    let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
    let state = codex2api_api::ApiState::new(
        storage.clone(),
        accounts,
        codex2api_upstream::UpstreamPool::new(auth),
    );
    for invalid in [
        "https://user:secret@example.test",
        "https://example.test/path",
        "https://example.test?query=1",
        "file:///tmp",
    ] {
        assert!(state.clone().with_public_base_url(invalid).is_err());
    }
    let configured = codex2api_api::router(
        state
            .with_public_base_url("https://proxy.example.test/")
            .unwrap(),
    );
    let response = configured
        .oneshot(
            Request::get(&metadata)
                .header("host", "internal:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        json_body(response).await["resource"],
        format!("https://proxy.example.test{ROOT}/backend-api/ps/mcp")
    );
    assert!(storage.missing_endpoints().await.unwrap().is_empty());
    assert_eq!(
        storage
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        0
    );
    storage.close().await;
}

#[tokio::test]
async fn virtual_workspace_config_is_independent_of_the_bound_upstream() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("config.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let path = format!("{ROOT}/backend-api/wham/config/bundle");
    assert_eq!(
        app.clone()
            .oneshot(Request::get(&path).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let tokens = login(&app).await;
    for bound in [true, false] {
        if !bound {
            bind_test_supplier(&storage, &account, None).await;
            storage.save_virtual_account(&account).await.unwrap();
        }
        let response = app
            .clone()
            .oneshot(
                Request::get(&path)
                    .header(
                        "authorization",
                        format!("Bearer {}", tokens["access_token"].as_str().unwrap()),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            json_body(response).await,
            serde_json::json!({"config_toml":{"enterprise_managed":[]},"requirements_toml":{"enterprise_managed":[]}})
        );
    }
    storage.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires Python and CODEX2API_TEST_CLI pointing to the installed desktop CLI"]
async fn installed_desktop_cli_completes_login_config_load_restart_and_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("desktop.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["model_access"] = serde_json::json!("selected");
    plan.config["models"] = serde_json::json!([{"provider_id":"chatgpt","model":"gpt-5.6-luna"},{"provider_id":"chatgpt","model":"gpt-6-astra"},{"provider_id":"chatgpt","model":"gpt-6-sol"},{"provider_id":"chatgpt","model":"gpt-6-luna"}]);
    for model in ["gpt-6-sol", "gpt-6-luna"] {
        sqlx::query("INSERT OR IGNORE INTO model_catalog(provider_id,model,kind) VALUES('chatgpt',?,'text')")
            .bind(model)
            .execute(storage.pool())
            .await
            .unwrap();
    }
    if std::env::var_os("CODEX2API_TEST_EMPTY_CATALOG").is_some() {
        plan.config["model_access"] = serde_json::json!("none");
        plan.config["models"] = serde_json::json!([]);
    }
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();

    if let Ok(directory) = std::env::var("CODEX2API_TEST_PUBLIC_RESOURCES") {
        let directory = std::path::Path::new(&directory);
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy();
            let resource = if name == "mcp-app.html" {
                Some(("/mcp-app.html".to_owned(), "text/html"))
            } else if name == "windows-store-update.json" {
                Some((
                    "/codex-app-prod/windows-store-update.json".to_owned(),
                    "application/json",
                ))
            } else if (name.ends_with(".js") || name.ends_with(".css"))
                && [
                    "main-BFDC70j-",
                    "apply-csp-",
                    "adapter-",
                    "run-widget-code-",
                    "main-foMHhJws",
                ]
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                Some((
                    format!("/assets/{name}"),
                    if name.ends_with(".css") {
                        "text/css"
                    } else {
                        "text/javascript"
                    },
                ))
            } else {
                None
            };
            if let Some((resource, kind)) = resource {
                let mut headers = serde_json::json!({"content-type":kind});
                if name == "mcp-app.html" {
                    let saved: Value = serde_json::from_slice(
                        &std::fs::read(directory.join("mcp-app.html.headers.json")).unwrap(),
                    )
                    .unwrap();
                    for key in ["Content-Security-Policy", "Permissions-Policy"] {
                        headers[key] = saved[key].clone();
                    }
                }
                storage
                    .save_desktop_resource(&resource, &std::fs::read(path).unwrap(), &headers)
                    .await
                    .unwrap();
            }
        }
    }
    record_usage(
        &storage,
        &account,
        test_supplier(&storage, &account).await.as_deref().unwrap(),
        "desktop-profile-fixture",
        12344,
    )
    .await;
    let profile = storage
        .virtual_config(&account.id, "profile")
        .await
        .unwrap();
    storage
        .update_virtual_config(
            &account.id,
            "profile",
            &serde_json::json!({"picture":null,"bio":"Virtual profile fixture"}),
            profile.revision,
        )
        .await
        .unwrap()
        .unwrap();
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    let seen = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<(String, u16)>::new()));
    let seen_by_router = seen.clone();
    let app = app.layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| {
            let seen = seen_by_router.clone();
            async move {
                let path = request.uri().path().to_owned();
                let response = next.run(request).await;
                seen.lock().await.push((path, response.status().as_u16()));
                response
            }
        },
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client_home = temp.path().join("client");
    std::fs::create_dir(&client_home).unwrap();
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/windows/Test-DesktopOAuth.py");
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("python")
            .arg(script)
            .arg(base)
            .arg(client_home)
            .output()
    })
    .await
    .unwrap()
    .unwrap();
    server.abort();
    storage.close().await;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        seen.lock().await.iter().any(|(path, status)| path
            == &format!("{ROOT}/backend-api/codex/models")
            && *status == 200),
        "The native model list must load this proxy's catalog rather than only local defaults."
    );

    if std::env::var_os("CODEX2API_TEST_DESKTOP_EXE").is_some() {
        let requests = seen.lock().await;
        for path in [
            "/backend-api/wham/usage",
            "/backend-api/wham/accounts/check",
            "/backend-api/settings/user",
        ] {
            assert!(
                requests
                    .iter()
                    .any(|(p, status)| p == &format!("{ROOT}{path}") && *status == 200),
                "Desktop did not load {path} successfully; observed paths and statuses: {requests:?}"
            );
        }
        println!(
            "Native desktop GUI: virtual workspace, settings and quota requests reached the local proxy successfully."
        );
        if std::env::var_os("CODEX2API_TEST_SUPPORT").is_some() {
            for path in [
                "/ces/v1/telemetry/intake",
                "/ces/v1/rgstr",
                "/v1/sdk_exception",
                "/v1/initialize",
                "/mcp-app.html",
            ] {
                assert!(
                    requests
                        .iter()
                        .any(|(p, s)| p == &format!("{ROOT}{path}") && (200..300).contains(s)),
                    "Desktop did not successfully request {path}: {requests:?}"
                );
            }
            assert!(
                requests
                    .iter()
                    .any(|(p, s)| p.starts_with(&format!("{ROOT}/assets/")) && *s == 200),
                "Desktop did not load MCP shell assets"
            );
        }
    }
}

#[tokio::test]
async fn desktop_login_wrapper_redirects_only_to_local_authorization_and_preserves_pkce() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("desktop-login.sqlite"))
        .await
        .unwrap();
    let (app, _) = fixture(&storage).await;
    let verifier = "v".repeat(43);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("response_type", "code"),
            ("scope", codex2api_version::OAUTH_SCOPE),
            ("client_id", codex2api_version::OAUTH_CLIENT_ID),
            ("redirect_uri", "http://localhost:1455/auth/callback"),
            ("state", "desktop-state"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("codex_streamlined_login", "true"),
            ("codex_app_version", "26.915.31029"),
        ])
        .finish();
    for origin in ["http://127.0.0.1:8080", "https://untrusted.example"] {
        let nested = format!("{origin}{ROOT}/oauth/authorize?{query}");
        let wrapper = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("authorize_url", &nested)
            .finish();
        let response=app.clone().oneshot(Request::get(format!("/codex/desktop-auth?{wrapper}&codex_streamlined_login=true&no_universal_links=1")).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response.headers()["location"].to_str().unwrap();
        assert_eq!(location, format!("{ROOT}/oauth/authorize?{query}"));
        assert_eq!(response.headers()["cache-control"], "no-store");
        let page = app
            .clone()
            .oneshot(Request::get(location).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            page.headers()["location"],
            format!("/admin/authorize/?{query}")
        );
    }
    for target in [
        "https://untrusted.example/steal".to_owned(),
        format!("http://127.0.0.1:8080{ROOT}/oauth/authorize?{query}#fragment"),
        format!("javascript:{ROOT}/oauth/authorize?{query}"),
        format!("http://127.0.0.1:8080{ROOT}/oauth/authorize?state=missing-pkce"),
    ] {
        let wrapper = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("authorize_url", &target)
            .finish();
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/codex/desktop-auth?{wrapper}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(!response.headers().contains_key("location"));
    }
    storage.close().await;
}

#[tokio::test]
async fn desktop_plugin_routes_require_virtual_auth_and_never_use_supplier_private_catalogs() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("plugins.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    for path in [
        "/backend-api/plugins/featured?platform=codex",
        "/backend-api/ps/plugins/list?scope=GLOBAL&limit=200",
        "/backend-api/ps/plugins/installed?limit=200&includeDownloadUrls=true",
        "/backend-api/ps/plugins/suggested/codex?scope=GLOBAL",
    ] {
        let uri = format!("{ROOT}{path}");
        let unauthenticated = app
            .clone()
            .oneshot(Request::get(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED, "{path}");
        let unbound = app
            .clone()
            .oneshot(
                Request::get(&uri)
                    .header(
                        "authorization",
                        format!("Bearer {}", tokens["access_token"].as_str().unwrap()),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if path.contains("/plugins/installed") {
            assert_eq!(unbound.status(), StatusCode::OK);
            assert_eq!(json_body(unbound).await["plugins"], serde_json::json!([]));
            continue;
        }
        assert_eq!(unbound.status(), StatusCode::NOT_IMPLEMENTED, "{path}");
        assert_eq!(
            json_body(unbound).await["error"]["code"],
            "catalog_authorization_unavailable"
        );
    }
    assert!(storage.missing_endpoints().await.unwrap().is_empty());
    assert_eq!(
        storage
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        0
    );
    storage.close().await;
}
async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}
async fn text_body(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}
fn form(path: &str, fields: &[(&str, &str)], cookie: Option<&str>) -> Request<Body> {
    if path == format!("{ROOT}/oauth/authorize") {
        let value: serde_json::Map<String, Value> = fields
            .iter()
            .map(|(key, value)| {
                (
                    match *key {
                        "flow" => "request_id",
                        "csrf" => "csrf_token",
                        other => other,
                    }
                    .to_owned(),
                    Value::String((*value).into()),
                )
            })
            .collect();
        let mut request = Request::post(format!("{ROOT}/oauth/authorize/submit"))
            .header("content-type", "application/json");
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        return request
            .body(Body::from(Value::Object(value).to_string()))
            .unwrap();
    }
    let mut fields = fields.to_vec();
    if path.starts_with("/admin/")
        && (fields.iter().any(|(k, _)| *k == "username")
            || fields.iter().any(|(k, _)| *k == "model_access"))
    {
        fields.retain(|(k, _)| *k != "account_id");
        if !fields.iter().any(|(k, _)| *k == "provider_id") {
            fields.push(("provider_id", "chatgpt"));
        }
        if fields.iter().any(|(k, _)| *k == "model_access")
            && !fields.iter().any(|(k, _)| *k == "free_model_access")
        {
            fields.push(("free_model_access", "none"));
        }
    }

    let mut request =
        Request::post(path).header("content-type", "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    request
        .body(Body::from(
            url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(fields.iter().copied())
                .finish(),
        ))
        .unwrap()
}
async fn seed_captured_config(
    storage: &Storage,
    owner: &str,
    key: &str,
    value: &Value,
    _expected: Option<i64>,
) -> codex2api_storage::Result<Option<i64>> {
    let current = storage.virtual_config(owner, key).await?;
    storage
        .update_virtual_config(owner, key, value, current.revision)
        .await
}
async fn admin_login(app: &Router) -> (String, String) {
    let response = app
        .clone()
        .oneshot(
            Request::post("/admin/api/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"username":"admin","password":"admin"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let data = json_body(response).await;
    (cookie, data["csrf_token"].as_str().unwrap().to_owned())
}
fn admin_request(
    method: &str,
    path: &str,
    cookie: &str,
    csrf: &str,
    value: Value,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("cookie", cookie)
        .header("x-csrf-token", csrf)
        .header("content-type", "application/json")
        .body(if value.is_null() {
            Body::empty()
        } else {
            Body::from(value.to_string())
        })
        .unwrap()
}
async fn admin_config(app: &Router, owner: &str, key: &str, cookie: &str) -> Value {
    let response = app
        .clone()
        .oneshot(admin_request(
            "GET",
            &format!("/admin/api/consumers/{owner}/configs"),
            cookie,
            "",
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == key)
        .unwrap()
        .clone()
}
async fn admin_save_config(
    app: &Router,
    owner: &str,
    key: &str,
    cookie: &str,
    csrf: &str,
    value: Value,
) {
    let config = admin_config(app, owner, key, cookie).await;
    let response = app
        .clone()
        .oneshot(admin_request(
            "PUT",
            &format!("/admin/api/consumers/{owner}/config/{key}"),
            cookie,
            csrf,
            serde_json::json!({"revision":config["revision"],"value":value}),
        ))
        .await
        .unwrap();
    let status = response.status();
    let body = text_body(response).await;
    assert_eq!(status, StatusCode::OK, "{key}: {body}");
}

async fn fixture(storage: &Storage) -> (Router, VirtualAccount) {
    let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
    let real = accounts.create_pending().await.unwrap().account;
    storage
        .update_account(
            &real.id,
            SupplierAccountUpdate {
                status: Some(SupplierStatus::Active),
                chatgpt_account_id: Some("real-workspace-secret".into()),
                email: Some("real-secret@example.test".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    accounts
        .save_auth_for_account(
            &real.id,
            &codex2api_accounts::AuthDotJson::chatgpt(
                codex2api_accounts::TokenData {
                    id_token: "real-id-secret".into(),
                    access_token: "real-token-secret".into(),
                    refresh_token: "real-refresh-secret".into(),
                    account_id: Some("real-workspace-secret".into()),
                },
                None,
            ),
        )
        .await
        .unwrap();
    let account = VirtualAccount {
        provider_id: "chatgpt".into(),
        id: uuid::Uuid::new_v4().to_string(),
        username: "alice".into(),
        password_hash: codex2api_storage::hash_password("fixture-password").unwrap(),
        name: "Alice Proxy".into(),
        email: "alice@example.test".into(),
        plan_type: "pro".into(),
        plan_id: "pro".into(),
        subscription_expires_at: Some("2027-01-01T00:00:00Z".into()),
        enabled: true,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_virtual_account(&account).await.unwrap();
    bind_test_supplier(storage, &account, Some(real.id)).await;
    (router(storage), account)
}
fn router(storage: &Storage) -> Router {
    let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
    let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
    codex2api_api::router(codex2api_api::ApiState::new(
        storage.clone(),
        accounts,
        codex2api_upstream::UpstreamPool::new(auth),
    ))
}

async fn authorize(app: &Router) -> (String, String, String, String) {
    authorize_scoped(app, codex2api_version::OAUTH_SCOPE).await
}
async fn authorize_scoped(app: &Router, scope: &str) -> (String, String, String, String) {
    let verifier = "v".repeat(43);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("response_type", "code"),
            ("scope", scope),
            ("client_id", codex2api_version::OAUTH_CLIENT_ID),
            ("redirect_uri", "http://localhost:1455/auth/callback"),
            ("state", "state-fixture"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ])
        .finish();
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("{ROOT}/oauth/authorize/bootstrap?{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let payload = json_body(response).await;
    (
        payload["request_id"].as_str().unwrap().into(),
        payload["csrf_token"].as_str().unwrap().into(),
        cookie,
        verifier,
    )
}
async fn login(app: &Router) -> Value {
    login_scoped(app, codex2api_version::OAUTH_SCOPE).await
}
async fn login_scoped(app: &Router, scope: &str) -> Value {
    let (flow, csrf, cookie, verifier) = authorize_scoped(app, scope).await;
    let fields = [
        ("flow", flow.as_str()),
        ("csrf", csrf.as_str()),
        ("username", "alice"),
        ("password", "fixture-password"),
    ];
    let response = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/authorize"),
            &fields,
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let callback = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    assert_eq!(callback.host_str(), Some("localhost"));
    let code = callback
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    let response = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/token"),
            &[
                ("grant_type", "authorization_code"),
                ("client_id", codex2api_version::OAUTH_CLIENT_ID),
                ("redirect_uri", "http://localhost:1455/auth/callback"),
                ("code", &code),
                ("code_verifier", &verifier),
            ],
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

#[tokio::test]
async fn browser_login_identity_quota_refresh_devices_and_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("virtual.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    let access = tokens["access_token"].as_str().unwrap();
    let refresh = tokens["refresh_token"].as_str().unwrap();
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
    assert_eq!(claims["email"], account.email);
    assert_eq!(
        claims["https://api.openai.com/auth"]["chatgpt_account_id"],
        account.id
    );
    assert!(!claims.to_string().contains("real-"));
    for (path, method) in [
        ("/backend-api/accounts/check/v4-2023-04-27", "GET"),
        ("/backend-api/wham/accounts/check", "GET"),
        ("/backend-api/wham/profiles/me", "GET"),
        ("/backend-api/subscriptions", "GET"),
        ("/backend-api/wham/usage", "GET"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(format!("{ROOT}{path}"))
                    .header("authorization", format!("Bearer {access}"))
                    .header("chatgpt-account-id", &account.id)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let value = json_body(response).await;
        assert!(!value.to_string().contains("real-"));
        if path.ends_with("/usage") {
            assert_eq!(value["plan_type"], "pro");
            assert!(value["rate_limit"]["primary_window"].is_null());
        }
    }
    let device = storage
        .virtual_devices(&account.id)
        .await
        .unwrap()
        .remove(0);
    assert!(device.last_used_at.is_some());
    storage
        .insert_usage(&UsageRecord {
            id: "usage-fixture".into(),
            account_id: test_supplier(&storage, &account).await.clone().unwrap(),
            subject_id: account.id.clone(),
            subject_name: account.name.clone(),
            requested_at_ms: chrono::Utc::now().timestamp_millis(),
            status: "in_progress".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut record = storage
        .query_usage(&Default::default())
        .await
        .unwrap()
        .records
        .remove(0);
    record.input_tokens = Some(100);
    record.status = "completed".into();
    storage.finish_usage(&record).await.unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("{ROOT}/backend-api/wham/usage"))
                .header("authorization", format!("Bearer {access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let displayed = json_body(response).await;
    assert!(displayed["rate_limit"]["primary_window"].is_null());
    assert_eq!(displayed["rate_limit"]["allowed"], true);
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("{ROOT}/unimplemented?access_token=secret-query"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    let records = storage.missing_endpoints().await.unwrap();
    assert_eq!(records.len(), 1);
    assert!(!records[0].path.contains("secret-query"));
    let refresh_request = || {
        form(
            &format!("{ROOT}/oauth/token"),
            &[("grant_type", "refresh_token"), ("refresh_token", refresh)],
            None,
        )
    };
    assert_eq!(
        app.clone()
            .oneshot(refresh_request())
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    storage
        .revoke_virtual_device(&account.id, &device.id)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(refresh_request())
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let tokens = login(&app).await;
    let refresh = tokens["refresh_token"].as_str().unwrap().to_string();
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    assert!(
        storage
            .virtual_refresh_device(&refresh)
            .await
            .unwrap()
            .is_some()
    );
    let mut edited = storage.virtual_account(&account.id).await.unwrap().unwrap();
    edited.password_hash = codex2api_storage::hash_password("new-password").unwrap();
    storage.save_virtual_account(&edited).await.unwrap();
    assert!(
        storage
            .virtual_refresh_device(&refresh)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .load_supplier_tokens(test_supplier(&storage, &account).await.as_deref().unwrap())
            .await
            .unwrap()
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("real-refresh-secret")
    );
    storage.close().await;
}

#[tokio::test]
async fn rejects_csrf_bad_password_redirects_and_pkce_replay() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("pkce.sqlite"))
        .await
        .unwrap();
    let (app, _) = fixture(&storage).await;
    let (flow, csrf, cookie, verifier) = authorize(&app).await;
    for (password, csrf, cookie) in [
        ("wrong", csrf.as_str(), Some(cookie.as_str())),
        ("fixture-password", "wrong", Some(cookie.as_str())),
        ("fixture-password", csrf.as_str(), None),
    ] {
        let response = app
            .clone()
            .oneshot(form(
                &format!("{ROOT}/oauth/authorize"),
                &[
                    ("flow", &flow),
                    ("csrf", csrf),
                    ("username", "alice"),
                    ("password", password),
                ],
                cookie,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let html = text_body(response).await;
        assert!(!html.contains("fixture-password"));
        if password == "wrong" {
            assert!(html.contains("用户名或密码错误"));
        }
    }
    let response = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/authorize"),
            &[
                ("flow", &flow),
                ("csrf", &csrf),
                ("username", "alice"),
                ("password", "fixture-password"),
            ],
            Some(&cookie),
        ))
        .await
        .unwrap();
    let url = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    let code = url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    for (proof, status) in [
        (&"x".repeat(43), StatusCode::BAD_REQUEST),
        (&verifier, StatusCode::OK),
        (&verifier, StatusCode::BAD_REQUEST),
    ] {
        let response = app
            .clone()
            .oneshot(form(
                &format!("{ROOT}/oauth/token"),
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("client_id", codex2api_version::OAUTH_CLIENT_ID),
                    ("redirect_uri", "http://localhost:1455/auth/callback"),
                    ("code_verifier", proof),
                ],
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    for redirect in [
        "http://evil.invalid:1455/auth/callback",
        "http://localhost.evil:1455/auth/callback",
        "http://localhost:1455/other",
        "http://user@localhost:1455/auth/callback",
        "http://localhost:1455/auth/callback?leak=x",
    ] {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("response_type", "code"),
                ("scope", codex2api_version::OAUTH_SCOPE),
                ("client_id", codex2api_version::OAUTH_CLIENT_ID),
                ("redirect_uri", redirect),
                ("state", "state"),
                ("code_challenge", &challenge),
                ("code_challenge_method", "S256"),
            ])
            .finish();
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("{ROOT}/oauth/authorize?{query}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(!response.headers().contains_key("location"));
    }
    storage.close().await;
}

async fn refresh(app: &Router, token: &str) -> axum::response::Response {
    app.clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/token"),
            &[("grant_type", "refresh_token"), ("refresh_token", token)],
            None,
        ))
        .await
        .unwrap()
}
async fn profile(app: &Router, token: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::get(format!("{ROOT}/backend-api/wham/profiles/me"))
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn desktop_profile_has_virtual_identity_and_persistent_local_statistics() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profile.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    let empty = json_body(profile(&app, token).await).await;
    assert_eq!(empty["profile"]["display_name"], account.name);
    assert_eq!(empty["profile"]["username"], account.username);
    assert_eq!(empty["stats"]["lifetime_tokens"], 0);
    assert_eq!(empty["stats"]["daily_usage_buckets"], serde_json::json!([]));
    assert!(empty["metadata"]["stats_error"].is_null());
    record_usage(
        &storage,
        &account,
        test_supplier(&storage, &account).await.as_deref().unwrap(),
        "first",
        12,
    )
    .await;
    let mut other = account.clone();
    other.id = "different-virtual-account".into();
    record_usage(
        &storage,
        &other,
        test_supplier(&storage, &account).await.as_deref().unwrap(),
        "foreign",
        999,
    )
    .await;
    record_usage(&storage, &account, "another-supplier", "second", 25).await;
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    let reopened = Storage::open(&path).await.unwrap();
    let value = json_body(profile(&router(&reopened), token).await).await;
    assert_eq!(value["profile"], empty["profile"]);
    assert_eq!(value["stats"]["lifetime_tokens"], 39);
    assert_eq!(value["stats"]["peak_daily_tokens"], 39);
    assert_eq!(value["stats"]["daily_usage_buckets"][0]["tokens"], 39);
    assert_eq!(value["stats"]["current_streak_days"], 1);
    assert!(value["stats"]["longest_running_turn_sec"].is_null());
    assert!(!value.to_string().contains("real-"));
    storage.record_virtual_analytics(&account.id, &[
        serde_json::json!({"event_type":"codex_turn_event","event_params":{"thread_id":"t1","turn_id":"r1"}}),
        serde_json::json!({"event_type":"codex_plugin_used","event_params":{"plugin_id":"plugin1","plugin_name":"Plugin"}})
    ]).await.unwrap();
    let page_path = "/backend-api/profiles/me/page";
    let page = json_body(
        router(&reopened)
            .oneshot(client_json("GET", page_path, token, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(page["page"]["stats"]["agentic"]["lifetime_tokens"], 39);
    assert_eq!(page["page"]["insights"]["agentic"]["total_threads"], 1);
    if let Ok(archive) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/windows/Test-DesktopProfileContract.cjs"),
            )
            .arg(archive)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(page.to_string().as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let updated = app
        .clone()
        .oneshot(client_json(
            "PATCH",
            page_path,
            token,
            serde_json::json!({"display_settings":{"show_insights_section":false}}),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        reopened
            .virtual_config(&account.id, "profile_page")
            .await
            .unwrap()
            .value["display_settings"]["show_insights_section"],
        false
    );
    let changed = app
        .clone()
        .oneshot(client_json(
            "PATCH",
            "/backend-api/profiles/me",
            token,
            serde_json::json!({"display_name":"Local profile","description":"Local biography"}),
        ))
        .await
        .unwrap();
    assert_eq!(changed.status(), StatusCode::OK);
    assert_eq!(json_body(changed).await["display_name"], "Local profile");
    assert_eq!(
        reopened
            .virtual_config(&account.id, "profile")
            .await
            .unwrap()
            .value["bio"],
        "Local biography"
    );
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/profiles/foreign/page",
                token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let workspace = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/accounts/check",
                token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        workspace["accounts"][0]["account_user_role"],
        "account-owner"
    );
    assert_eq!(workspace["accounts"][0]["is_zdr"], false);
    assert_eq!(workspace["accounts"][0]["is_openai_internal"], false);
    assert_eq!(
        workspace["accounts"][0]["workspace_backend_origin"],
        "NO_CONSTRAINT"
    );
}
async fn record_usage(
    storage: &Storage,
    owner: &VirtualAccount,
    real: &str,
    id: &str,
    tokens: i64,
) {
    let mut row = UsageRecord {
        id: id.into(),
        account_id: real.into(),
        account_name: real.into(),
        subject_id: owner.id.clone(),
        subject_name: owner.name.clone(),
        requested_at_ms: chrono::Utc::now().timestamp_millis(),
        status: "in_progress".into(),
        ..Default::default()
    };
    storage.insert_usage(&row).await.unwrap();
    row.input_tokens = Some(tokens);
    row.output_tokens = Some(1);
    row.status = "completed".into();
    storage.finish_usage(&row).await.unwrap();
}

#[tokio::test]
async fn remote_host_registration_refresh_socket_and_device_revocation_are_isolated() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{Message, client::IntoClientRequest},
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("remote.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
    let payload = serde_json::json!({"name":"Desktop host","os":"windows","arch":"x86_64","app_server_version":"fixture","installation_id":"install-one"});
    let enroll_path = "/backend-api/wham/remote/control/server/enroll";
    let response = app
        .clone()
        .oneshot(client_json("POST", enroll_path, token, payload.clone()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let first = json_body(response).await;
    let second = json_body(
        app.clone()
            .oneshot(client_json("POST", enroll_path, token, payload))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(first["server_id"], second["server_id"]);
    assert_eq!(first["environment_id"], second["environment_id"]);
    assert!(
        storage
            .remote_server_for_token(first["remote_control_token"].as_str().unwrap())
            .await
            .unwrap()
            .is_none()
    );
    let mut other = account.clone();
    other.id = "foreign-owner".into();
    other.username = "foreign".into();
    storage.save_virtual_account(&other).await.unwrap();
    assert!(storage.remote_servers(&other.id).await.unwrap().is_empty());
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    let reopened = Storage::open(&path).await.unwrap();
    assert_eq!(reopened.remote_servers(&account.id).await.unwrap().len(), 1);
    let refreshed = app
        .clone()
        .oneshot(client_json(
            "POST",
            "/backend-api/wham/remote/control/server/refresh",
            token,
            serde_json::json!({"server_id":first["server_id"],"installation_id":"install-one"}),
        ))
        .await
        .unwrap();
    assert_eq!(refreshed.status(), StatusCode::OK);
    let refreshed = json_body(refreshed).await;
    let remote_token = refreshed["remote_control_token"].as_str().unwrap();
    assert!(
        storage
            .remote_server_for_token(second["remote_control_token"].as_str().unwrap())
            .await
            .unwrap()
            .is_none()
    );
    let mismatch = app
        .clone()
        .oneshot(client_json(
            "POST",
            "/backend-api/wham/remote/control/server/refresh",
            token,
            serde_json::json!({"server_id":first["server_id"],"installation_id":"another-install"}),
        ))
        .await
        .unwrap();
    assert_eq!(mismatch.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/profiles/me/page",
                remote_token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut request = format!("ws://{address}{ROOT}/backend-api/wham/remote/control/server")
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "authorization",
        format!("Bearer {remote_token}").parse().unwrap(),
    );
    request.headers_mut().insert(
        "x-codex-server-id",
        first["server_id"].as_str().unwrap().parse().unwrap(),
    );
    request
        .headers_mut()
        .insert("x-codex-installation-id", "install-one".parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let ping = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(ping, Message::Ping(_)));
    socket.send(Message::Ping(vec![1].into())).await.unwrap();
    let pong = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(pong, Message::Pong(_)));
    let saved = storage.remote_servers(&account.id).await.unwrap().remove(0);
    assert!(saved.connected_until_ms > chrono::Utc::now().timestamp_millis());
    storage
        .revoke_virtual_device(&account.id, &saved.device_id)
        .await
        .unwrap();
    assert!(
        storage
            .remote_server_for_token(remote_token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .remote_servers(&account.id)
            .await
            .unwrap()
            .is_empty()
    );
    let close = tokio::time::timeout(std::time::Duration::from_secs(12), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(close, Message::Close(_)));
    server.abort();
}

#[tokio::test]
async fn binding_changes_and_upstream_deletion_preserve_identity_devices_and_both_usage_dimensions()
{
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("binding.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let (app, account) = fixture(&storage).await;
    let tokens = login(&app).await;
    let access = tokens["access_token"].as_str().unwrap();
    let rt = tokens["refresh_token"].as_str().unwrap();
    let devices = storage.virtual_devices(&account.id).await.unwrap();
    let first = test_supplier(&storage, &account).await.clone().unwrap();
    record_usage(&storage, &account, &first, "before", 12).await;
    let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
    let second = accounts.create_pending().await.unwrap().account;
    storage
        .update_account(
            &second.id,
            SupplierAccountUpdate {
                status: Some(SupplierStatus::Active),
                chatgpt_account_id: Some("second-upstream".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    accounts
        .save_auth_for_account(
            &second.id,
            &codex2api_accounts::AuthDotJson::chatgpt(
                codex2api_accounts::TokenData {
                    id_token: "fixture-id".into(),
                    access_token: "fixture-access".into(),
                    refresh_token: "fixture-refresh".into(),
                    account_id: Some("second-upstream".into()),
                },
                None,
            ),
        )
        .await
        .unwrap();
    bind_test_supplier(&storage, &account, Some(second.id.clone())).await;
    storage.save_virtual_account(&account).await.unwrap();
    assert_eq!(
        storage.virtual_devices(&account.id).await.unwrap()[0].id,
        devices[0].id
    );
    let resolved = storage
        .virtual_access(&codex2api_storage::hash_token(access))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved.account_id.as_deref(), Some(second.id.as_str()));
    assert_eq!(resolved.virtual_account_id, account.id);
    record_usage(&storage, &account, &second.id, "after", 25).await;
    assert_eq!(
        storage.virtual_usage_tokens(&account.id, 0).await.unwrap(),
        39
    );
    let before = storage.account_usage_summary(&first).await.unwrap();
    let after = storage.account_usage_summary(&second.id).await.unwrap();
    assert_eq!(before.lifetime_tokens, Some(13));
    assert_eq!(after.lifetime_tokens, Some(26));
    sqlx::query("DELETE FROM supplier_accounts WHERE id=?")
        .bind(&second.id)
        .execute(storage.pool())
        .await
        .unwrap();
    assert!(
        storage
            .execution_route(&account.id, "chatgpt")
            .await
            .unwrap()
            .unwrap()
            .supplier_account_id
            .is_none()
    );
    assert_eq!(storage.virtual_devices(&account.id).await.unwrap().len(), 1);
    assert_eq!(
        json_body(profile(&app, access).await).await["profile"]["id"],
        account.id
    );
    assert_eq!(refresh(&app, rt).await.status(), StatusCode::OK);
    let denied = app
        .clone()
        .oneshot(
            Request::post(format!("{ROOT}/backend-api/codex/responses"))
                .header("authorization", format!("Bearer {access}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::SERVICE_UNAVAILABLE);
    drop(app);
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    let app = router(&storage);
    assert_eq!(refresh(&app, rt).await.status(), StatusCode::OK);
    assert_eq!(
        storage.virtual_usage_tokens(&account.id, 0).await.unwrap(),
        39
    );
    let mut account = storage.virtual_account(&account.id).await.unwrap().unwrap();
    bind_test_supplier(&storage, &account, Some(first)).await;
    storage.save_virtual_account(&account).await.unwrap();
    assert_eq!(refresh(&app, rt).await.status(), StatusCode::OK);
    assert_eq!(
        storage
            .account_usage_summary(&second.id)
            .await
            .unwrap()
            .lifetime_tokens,
        Some(26)
    );
    account.enabled = false;
    storage.save_virtual_account(&account).await.unwrap();
    assert_eq!(refresh(&app, rt).await.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        profile(&app, access).await.status(),
        StatusCode::UNAUTHORIZED
    );
    account.enabled = true;
    storage.save_virtual_account(&account).await.unwrap();
    assert_eq!(refresh(&app, rt).await.status(), StatusCode::BAD_REQUEST);
    let fresh = login(&app).await;
    storage.delete_virtual_account(&account.id).await.unwrap();
    assert_eq!(
        refresh(&app, fresh["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        profile(&app, fresh["access_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        storage.virtual_usage_tokens(&account.id, 0).await.unwrap(),
        39
    );
    storage.close().await;
}

#[tokio::test]
async fn authorization_codes_expire_bind_client_and_callback_and_redeem_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("codes.sqlite"))
        .await
        .unwrap();
    let (app, _) = fixture(&storage).await;
    let (flow, csrf, cookie, verifier) = authorize(&app).await;
    let response = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/authorize"),
            &[
                ("flow", &flow),
                ("csrf", &csrf),
                ("username", "alice"),
                ("password", "fixture-password"),
            ],
            Some(&cookie),
        ))
        .await
        .unwrap();
    let callback = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    assert_eq!(
        callback
            .query_pairs()
            .find(|(k, _)| k == "state")
            .unwrap()
            .1,
        "state-fixture"
    );
    let code = callback
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let device_info = codex2api_storage::OAuthDeviceIdentity::default();
    assert!(
        storage
            .redeem_oauth_code(codex2api_storage::CodeRedemption {
                provider: "chatgpt",
                code: &code,
                client_id: "wrong",
                redirect_uri: "http://localhost:1455/auth/callback",
                challenge: &challenge,
                refresh: "fixture-refresh",
                device: &device_info
            })
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .redeem_oauth_code(codex2api_storage::CodeRedemption {
                provider: "chatgpt",
                code: &code,
                client_id: codex2api_version::OAUTH_CLIENT_ID,
                redirect_uri: "http://localhost:1456/auth/callback",
                challenge: &challenge,
                refresh: "fixture-refresh",
                device: &device_info
            })
            .await
            .unwrap()
            .is_none()
    );
    let (one, two) = tokio::join!(
        storage.redeem_oauth_code(codex2api_storage::CodeRedemption {
            provider: "chatgpt",
            code: &code,
            client_id: codex2api_version::OAUTH_CLIENT_ID,
            redirect_uri: "http://localhost:1455/auth/callback",
            challenge: &challenge,
            refresh: "fixture-refresh",
            device: &device_info
        }),
        storage.redeem_oauth_code(codex2api_storage::CodeRedemption {
            provider: "chatgpt",
            code: &code,
            client_id: codex2api_version::OAUTH_CLIENT_ID,
            redirect_uri: "http://localhost:1455/auth/callback",
            challenge: &challenge,
            refresh: "fixture-refresh",
            device: &device_info
        })
    );
    assert_eq!(
        usize::from(one.unwrap().is_some()) + usize::from(two.unwrap().is_some()),
        1
    );
    let (flow, csrf, cookie, verifier) = authorize(&app).await;
    let response = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/authorize"),
            &[
                ("flow", &flow),
                ("csrf", &csrf),
                ("username", "alice"),
                ("password", "fixture-password"),
            ],
            Some(&cookie),
        ))
        .await
        .unwrap();
    let callback = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    let code = callback
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    sqlx::query("UPDATE virtual_authorization_codes SET expires_at=0")
        .execute(storage.pool())
        .await
        .unwrap();
    let response = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/token"),
            &[
                ("grant_type", "authorization_code"),
                ("client_id", codex2api_version::OAUTH_CLIENT_ID),
                ("redirect_uri", "http://localhost:1455/auth/callback"),
                ("code", &code),
                ("code_verifier", &verifier),
            ],
            None,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(storage.virtual_accounts().await.unwrap().len() == 1);
    storage.close().await;
}

#[tokio::test]
async fn credentials_are_device_scoped_and_identity_and_diagnostic_boundaries_hold() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("boundaries.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let first = login(&app).await;
    let second = login(&app).await;
    assert_ne!(first["refresh_token"], second["refresh_token"]);
    assert_eq!(storage.virtual_devices(&account.id).await.unwrap().len(), 2);
    let access = first["access_token"].as_str().unwrap();
    let mismatch = app
        .clone()
        .oneshot(
            Request::get(format!("{ROOT}/backend-api/wham/profiles/me"))
                .header("authorization", format!("Bearer {access}"))
                .header(
                    "chatgpt-account-id",
                    test_supplier(&storage, &account).await.as_deref().unwrap(),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mismatch.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        profile(&app, &format!("{access}tampered")).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let revoked = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/revoke"),
            &[("token", first["refresh_token"].as_str().unwrap())],
            None,
        ))
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::OK);
    assert_eq!(
        profile(&app, access).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        refresh(&app, second["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::OK
    );
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(
                Request::post(format!(
                    "{ROOT}/not-yet-implemented?password=never-record-this"
                ))
                .header("authorization", "Bearer never-record-this")
                .body(Body::from("never-record-this"))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
        assert_eq!(
            json_body(response).await["error"]["code"],
            "endpoint_not_implemented"
        );
    }
    let diagnostics = storage.missing_endpoints().await.unwrap();
    assert_eq!(diagnostics[0].hits, 2);
    assert_eq!(diagnostics[0].method, "POST");
    assert_eq!(diagnostics[0].path, format!("{ROOT}/not-yet-implemented"));
    let (flow, csrf, cookie, _) = authorize(&app).await;
    sqlx::query("UPDATE oauth_browser_flows SET expires_at=0 WHERE id=?")
        .bind(&flow)
        .execute(storage.pool())
        .await
        .unwrap();
    let expired = app
        .clone()
        .oneshot(form(
            &format!("{ROOT}/oauth/authorize"),
            &[
                ("flow", &flow),
                ("csrf", &csrf),
                ("username", "alice"),
                ("password", "fixture-password"),
            ],
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(expired.status(), StatusCode::BAD_REQUEST);
    storage.close().await;
}

#[tokio::test]
async fn repairs_expiry_plan_change_and_cost_guards_use_current_entitlements() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("repairs.sqlite"))
        .await
        .unwrap();
    let (app, mut account) = fixture(&storage).await;
    account.subscription_expires_at =
        Some((chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339());
    storage
        .save_virtual_account_operation(&account, "admin")
        .await
        .unwrap();
    let tokens = login(&app).await;
    let token = tokens["access_token"].as_str().unwrap();
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
    assert_eq!(
        claims["https://api.openai.com/auth"]["chatgpt_plan_type"],
        "free"
    );
    for (path, pointer) in [
        ("/backend-api/wham/usage", "/plan_type"),
        (
            "/backend-api/accounts/optimized/check",
            "/account/plan_type",
        ),
        ("/backend-api/me", "/plan_type"),
        ("/backend-api/subscriptions", "/plan_type"),
    ] {
        let response = app
            .clone()
            .oneshot(client_json("GET", path, token, Value::Null))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await.pointer(pointer),
            Some(&json!("free")),
            "{path}"
        );
    }
    let request = json!({"new_task":{},"input_items":[]});
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/tasks",
                token,
                request.clone()
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    account.subscription_expires_at =
        Some((chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339());
    storage
        .save_virtual_account_operation(&account, "admin")
        .await
        .unwrap();
    let revision = storage
        .virtual_config(&account.id, "subscription_entitlements")
        .await
        .unwrap()
        .revision;
    let mut plans = storage
        .virtual_config(&account.id, "subscription_entitlements")
        .await
        .unwrap()
        .value;
    plans["plus"]["primary_cost_limit_usd"] = json!(0);
    plans["pro"]["weekly_cost_limit_usd"] = json!(10);
    set_test_plan_config(
        &storage,
        &account.id,
        "subscription_entitlements",
        &plans,
        revision,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/tasks",
                token,
                request.clone()
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    account.plan_type = "plus".into();
    account.plan_id = "plus".into();
    storage
        .save_virtual_account_operation(&account, "admin")
        .await
        .unwrap();
    // 用量记录启动内层窗口，随后验证其零额度在所有执行入口生效。
    storage
        .insert_usage(&UsageRecord {
            id: "prior-plan-request".into(),
            account_id: test_supplier(&storage, &account).await.unwrap(),
            subject_id: account.id.clone(),
            endpoint: "/v1/responses".into(),
            transport: "http".into(),
            requested_at_ms: chrono::Utc::now().timestamp_millis(),
            status: "in_progress".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let quota = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/usage",
                token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(quota["plan_type"], "plus");
    assert_eq!(quota["rate_limit"]["allowed"], false);
    assert_eq!(quota["rate_limit"]["primary_window"]["used_percent"], 100);
    for path in [
        "/backend-api/wham/tasks",
        "/backend-api/codex/realtime/calls",
        "/backend-api/codex/responses",
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json(
                    "POST",
                    path,
                    token,
                    if path.contains("realtime") {
                        json!({"sdp":"v=0","session":{"model":"gpt-6-astra"}})
                    } else {
                        request.clone()
                    }
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::TOO_MANY_REQUESTS,
            "{path}"
        );
    }
    let policy = storage
        .virtual_config(&account.id, "subscription_entitlements")
        .await
        .unwrap();
    let mut plans = policy.value;
    plans["plus"]["primary_cost_limit_usd"] = Value::Null;
    plans["plus"]["models"] = json!(["allowed-model"]);
    set_test_plan_config(
        &storage,
        &account.id,
        "subscription_entitlements",
        &plans,
        policy.revision,
    )
    .await
    .unwrap();
    let rejected = app
        .clone()
        .oneshot(client_json(
            "POST",
            "/backend-api/codex/responses",
            token,
            json!({"model":"not-entitled","input":[]}),
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(rejected).await["error"]["code"],
        "model_not_entitled"
    );
    let models = storage.virtual_config(&account.id, "models").await.unwrap();
    let mut catalog = models.value;
    catalog["models"] = json!([{"slug":"allowed-model"},{"slug":"not-entitled"}]);
    storage
        .update_virtual_config(&account.id, "models", &catalog, models.revision)
        .await
        .unwrap();
    let catalog = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/models",
                token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(catalog["models"].as_array().unwrap().len(), 1);
    assert_eq!(catalog["models"][0]["slug"], "allowed-model");
    assert!(
        storage
            .virtual_resources(&account.id, "subscription_operation")
            .await
            .unwrap()
            .iter()
            .any(|v| v["operation"] == "renew")
    );
    assert!(
        storage
            .virtual_access(&codex2api_storage::hash_token(token))
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn repairs_nested_task_and_turn_ownership_mcp_and_rebinding_are_checked_before_execution() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("isolation.sqlite"))
        .await
        .unwrap();
    let (app, account) = fixture(&storage).await;
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut other = account.clone();
    other.id = "other-owner".into();
    other.username = "other-owner".into();
    storage.save_virtual_account(&other).await.unwrap();
    storage
        .save_virtual_resource(
            &other.id,
            "task",
            "foreign",
            test_supplier(&storage, &account).await.as_deref(),
            &json!({"task":{"id":"foreign"}}),
        )
        .await
        .unwrap();
    storage
        .save_virtual_resource(
            &account.id,
            "task",
            "own",
            test_supplier(&storage, &account).await.as_deref(),
            &json!({"task":{"id":"own"}}),
        )
        .await
        .unwrap();
    storage
        .save_virtual_resource(
            &account.id,
            "task_turn",
            "own:turn",
            test_supplier(&storage, &account).await.as_deref(),
            &json!({"id":"turn"}),
        )
        .await
        .unwrap();
    let quota = storage.virtual_config(&account.id, "quota").await.unwrap();
    set_test_plan_config(
        &storage,
        &account.id,
        "quota",
        &json!({"primary_cost_limit_usd":0,"weekly_cost_limit_usd":null}),
        quota.revision,
    )
    .await
    .unwrap();
    for (task, turn, status) in [
        ("foreign", "turn", StatusCode::NOT_FOUND),
        ("own", "foreign-turn", StatusCode::NOT_FOUND),
        ("own", "turn", StatusCode::TOO_MANY_REQUESTS),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json(
                    "POST",
                    "/backend-api/wham/tasks",
                    &token,
                    json!({"follow_up":{"task_id":task,"turn_id":turn},"input_items":[]})
                ))
                .await
                .unwrap()
                .status(),
            status
        );
    }
    for path in [
        "/backend-api/wham/tasks/foreign/turns",
        "/backend-api/wham/tasks/own/turns/foreign-turn/logs",
        "/backend-api/conversation/foreign",
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json("GET", path, &token, Value::Null))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    for tool in ["sites.get_environment_variables", "sites.delete_site"] {
        let response=app.clone().oneshot(client_json("POST","/backend-api/ps/mcp",&token,json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":tool,"arguments":{"project_id":"supplier-private","secret":"DO_NOT_LOG"}}}))).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
        assert_eq!(json_body(response).await["id"], 7);
    }
    let operations = storage
        .virtual_resources(&account.id, "mcp_operation")
        .await
        .unwrap();
    assert_eq!(operations.len(), 2);
    assert!(
        !serde_json::to_string(&operations)
            .unwrap()
            .contains("DO_NOT_LOG")
    );
    assert!(
        storage
            .virtual_resources(&other.id, "mcp_operation")
            .await
            .unwrap()
            .is_empty()
    );
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/tasks",
                &token,
                json!({"follow_up":{"task_id":"own","turn_id":"turn"}})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.oneshot(client_json(
            "GET",
            "/backend-api/wham/tasks/own",
            &token,
            Value::Null
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn repairs_client_writes_nonempty_pages_and_events_stay_owned_and_survive_restart() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("client-state.sqlite");
    let storage = Storage::open(&db).await.unwrap();
    let (app, account) = fixture(&storage).await;
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut other = account.clone();
    other.id = "other-records".into();
    other.username = "other-records".into();
    storage.save_virtual_account(&other).await.unwrap();
    bind_test_supplier(&storage, &account, None).await;
    storage.save_virtual_account(&account).await.unwrap();
    for (id, archived, starred, time) in [
        ("current", false, true, 20),
        ("archived", true, false, 30),
        ("second", false, false, 10),
    ] {
        storage.save_virtual_resource(&account.id,"conversation",id,None,&json!({"id":id,"is_archived":archived,"is_starred":starred,"update_time":time,"create_time":time,"conversation_origin":"tpp"})).await.unwrap();
    }
    storage
        .save_virtual_resource(
            &other.id,
            "conversation",
            "foreign",
            None,
            &json!({"id":"foreign"}),
        )
        .await
        .unwrap();
    for (id, archived, at, status) in [
        ("task-current", false, 30, "in_progress"),
        ("task-archived", true, 40, "completed"),
        ("task-older", false, 9, "pending"),
    ] {
        storage.save_virtual_resource(&account.id,"task",id,None,&json!({"task":{"id":id,"archived":archived,"updated_at":at},"current_assistant_turn":{"turn_status":status}})).await.unwrap();
    }
    let tasks = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/tasks/list?task_filter=current&limit=1",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(tasks["items"][0]["id"], "task-current");
    assert_eq!(
        tasks["items"][0]["task_status_display"]["latest_turn_status_display"]["turn_status"],
        "in_progress"
    );
    assert_eq!(tasks["cursor"], "1");
    let tasks = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/tasks/list?task_filter=current&limit=1&cursor=1",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(tasks["items"][0]["id"], "task-older");
    assert!(tasks["cursor"].is_null());
    for (path, expected) in [
        (
            "/backend-api/conversations?is_archived=false&limit=1&order=updated",
            "current",
        ),
        (
            "/backend-api/conversations?is_archived=false&offset=1&limit=1",
            "second",
        ),
        ("/backend-api/conversations?is_starred=true", "current"),
        ("/backend-api/conversations?is_archived=true", "archived"),
    ] {
        let response = json_body(
            app.clone()
                .oneshot(client_json("GET", path, &token, Value::Null))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(response["items"].as_array().unwrap().len(), 1);
        assert_eq!(response["items"][0]["id"], expected);
    }
    for (method, path, status) in [
        (
            "POST",
            "/backend-api/pins/conversation/current",
            StatusCode::OK,
        ),
        (
            "POST",
            "/backend-api/pins/conversation/foreign",
            StatusCode::NOT_FOUND,
        ),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json(method, path, &token, Value::Null))
                .await
                .unwrap()
                .status(),
            status
        );
    }
    let pins = json_body(
        app.clone()
            .oneshot(client_json("GET", "/backend-api/pins", &token, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(pins[0]["item_id"], "current");
    let preferences = json_body(
        app.clone()
            .oneshot(client_json(
                "PATCH",
                "/backend-api/wham/settings/user",
                &token,
                json!({"branch_format":"work/{task_id}","git_diff_mode":"split"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(preferences["git_diff_mode"], "split");
    let cloud = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/settings/user",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(cloud, preferences);
    let schema = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/wham/settings/configs/user-preferences",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "PATCH",
                "/backend-api/settings/account_user_setting?feature=voice_name&value=marin",
                &token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let settings = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/settings/user",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(settings["settings"]["voice_name"], "marin");
    let config = storage
        .virtual_config(&account.id, "automations")
        .await
        .unwrap();
    storage.update_virtual_config(&account.id,"automations",&json!({"items":[{"id":"one","is_enabled":true,"updated_at":"2026-09-21"},{"id":"two","is_enabled":true,"updated_at":"2026-09-20"},{"id":"paused","is_enabled":false}],"cursor":"stale-fixed-cursor"}),config.revision).await.unwrap();
    let first = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/automations?filter=scheduled&limit=1",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(first["cursor"], "1");
    let second = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/automations?filter=scheduled&limit=1&cursor=1",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(second["items"][0]["id"], "two");
    assert!(second["cursor"].is_null());
    storage
        .save_virtual_resource(
            &account.id,
            "conversation",
            "current",
            None,
            &json!({"id":"current","status":"finished_successfully","update_time":40}),
        )
        .await
        .unwrap();
    let events = storage
        .virtual_events(&account.id, 0)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e["topic"] == "conversations")
        .collect::<Vec<_>>();
    assert!(
        events
            .iter()
            .any(|e| e["payload"]["type"] == "conversation-turn-complete")
    );
    assert!(!serde_json::to_string(&events).unwrap().contains("foreign"));
    if let Ok(source) = std::env::var("CODEX2API_TEST_DESKTOP_SOURCE") {
        run_repair_contract(
            &source,
            json!({"automation_pages":[first,second],"cloud_preferences":cloud,"cloud_schema":schema,"events":events}),
        );
    }
    storage.close().await;
    let reopened = Storage::open(&db).await.unwrap();
    assert_eq!(
        reopened
            .virtual_config(&account.id, "voice")
            .await
            .unwrap()
            .value["selected"],
        "marin"
    );
    assert_eq!(
        reopened
            .virtual_config(&account.id, "pins")
            .await
            .unwrap()
            .write_origin,
        "client"
    );
    assert!(
        reopened
            .virtual_config(&other.id, "pins")
            .await
            .unwrap()
            .value
            .as_array()
            .unwrap()
            .is_empty()
    );
}

fn run_repair_contract(source: &str, sample: Value) {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut process = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scripts/windows/Test-DesktopVirtualRepairs.mjs"),
        )
        .arg(source)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(sample.to_string().as_bytes())
        .unwrap();
    let output = process.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn admin_json_operations_persist_valid_nonempty_desktop_configuration() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("admin-client.sqlite"))
        .await
        .unwrap();
    let (api, account) = fixture(&storage).await;
    storage.ensure_default_admin().await.unwrap();
    let app = api.merge(codex2api_admin::router(
        codex2api_admin::AdminState::new(storage.clone()).unwrap(),
    ));
    let (cookie, csrf) = admin_login(&app).await;
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let installed = storage
        .virtual_config(&account.id, "installed_plugins")
        .await
        .unwrap();
    storage
        .update_virtual_config(
            &account.id,
            "installed_plugins",
            &json!({"plugins":[{"id":"example-plugin","name":"Example plugin"}]}),
            installed.revision,
        )
        .await
        .unwrap();
    storage
        .save_virtual_resource(
            &account.id,
            "connector_catalog",
            "example-connector",
            None,
            &json!({"id":"example-connector","name":"Example connector"}),
        )
        .await
        .unwrap();
    let configs = [
        (
            "models",
            json!({"models":[{"slug":"example-model","title":"Example","description":"A model","enabled_tools":["browser"],"configurable_thinking_effort":true,"thinking_efforts":[{"thinking_effort":"standard","full_label":"Standard"}],"default_thinking_effort":"standard","product_features":{"attachments":{"type":"multimodal","accepted_mime_types":["image/png"],"image_mime_types":["image/png"],"can_accept_all_mime_types":false}}}],"categories":[],"versions":[{"display_text":"Example version","name":"Example version","slugs":["example-model"],"intelligence_presets":[{"model_slug":"example-model","lane":"thinking","thinking_effort":"standard","title":"Standard"}]}],"internal_groups":[],"slider_settings":[],"default_model_slug":"example-model"}),
        ),
        (
            "system_hints",
            json!({"system_hints":[{"title":"Plugin hint","description":"Use this plugin","hint_kind":"plugin","resource_id":"example-plugin"},{"title":"Connector hint","description":"Use this connector","hint_kind":"connector","resource_id":"example-connector"},{"title":"Basic hint","description":"A basic hint","hint_kind":"basic","resource_id":""}]}),
        ),
        (
            "beacons",
            json!({"beacon_ui_response":{"title":"Service news","description":"An actual configured announcement","presentation":"banner","buttons":[{"text":"Read more","action":"open_url","url":"https://example.test/news"},{"text":"Close","action":"dismiss","url":""}]}}),
        ),
    ];
    sqlx::query("INSERT INTO model_catalog(provider_id,model,kind) VALUES('chatgpt','example-model','text')").execute(storage.pool()).await.unwrap();
    for (key, value) in &configs {
        if *key == "models" {
            let old = storage.virtual_config(&account.id, key).await.unwrap();
            let mut prepared = value.clone();
            codex2api_storage::prepare_virtual_contract(key, &mut prepared);
            storage
                .update_virtual_config(&account.id, key, &prepared, old.revision)
                .await
                .unwrap();
            assert_eq!(
                admin_config(&app, &account.id, key, &cookie).await["readonly"],
                true
            );
        } else {
            admin_save_config(&app, &account.id, key, &cookie, &csrf, value.clone()).await;
            assert_eq!(
                storage
                    .virtual_config(&account.id, key)
                    .await
                    .unwrap()
                    .write_origin,
                "admin"
            );
        }
    }
    let expiry = (chrono::Utc::now() + chrono::Duration::days(60)).to_rfc3339();
    let saved=app.clone().oneshot(admin_request("PUT",&format!("/admin/api/consumers/{}",account.id),&cookie,&csrf,json!({"provider_id":"chatgpt","username":account.username,"name":account.name,"email":account.email,"plan_id":"plus","subscription_expires_at":expiry,"enabled":true}))).await.unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    let identity = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/accounts/optimized/check",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(identity["account"]["plan_type"], "plus");
    assert_eq!(identity["entitlement"]["has_active_subscription"], true);
    assert!(
        storage
            .virtual_resources(&account.id, "subscription_operation")
            .await
            .unwrap()
            .iter()
            .any(|v| v["origin"] == "admin" && v["operation"] == "change_plan")
    );
    let records = json_body(
        app.clone()
            .oneshot(admin_request(
                "GET",
                &format!(
                    "/admin/api/consumers/{}/records?kind=subscription_operation",
                    account.id
                ),
                &cookie,
                &csrf,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(records.to_string().contains("change_plan"));
    let read = |path: &str| client_json("GET", path, &token, Value::Null);
    let models = json_body(
        app.clone()
            .oneshot(read("/backend-api/models"))
            .await
            .unwrap(),
    )
    .await;
    let hints_plugins = json_body(
        app.clone()
            .oneshot(read("/backend-api/system_hints?mode=plugins"))
            .await
            .unwrap(),
    )
    .await;
    let hints_connectors = json_body(
        app.clone()
            .oneshot(read("/backend-api/system_hints?mode=connectors"))
            .await
            .unwrap(),
    )
    .await;
    let hints_basic = json_body(
        app.clone()
            .oneshot(read("/backend-api/system_hints?mode=basic"))
            .await
            .unwrap(),
    )
    .await;
    let beacons = json_body(
        app.clone()
            .oneshot(read("/backend-api/beacons/home"))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        models["versions"][0]["id"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    assert_eq!(
        hints_plugins["system_hints"][0]["system_hint"],
        "plugin:example-plugin"
    );
    assert_eq!(
        hints_connectors["system_hints"].as_array().unwrap().len(),
        1
    );
    assert_eq!(hints_basic["system_hints"].as_array().unwrap().len(), 1);
    assert_eq!(
        beacons["beacon_ui_response"]["ui_info"]["title"],
        "Service news"
    );
    let sample = json!({"models":models,"hints":{"plugins":hints_plugins,"connectors":hints_connectors,"basic":hints_basic},"beacons":beacons});
    if let Ok(source) = std::env::var("CODEX2API_TEST_DESKTOP_SOURCE") {
        run_repair_contract(&source, sample.clone());
    }
    if let Ok(output) = std::env::var("CODEX2API_REPAIR_OUTPUT") {
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(
            std::path::Path::new(&output).join("admin-api-client.json"),
            sample.to_string(),
        )
        .unwrap();
    }
    let mut invalid = storage
        .virtual_config(&account.id, "models")
        .await
        .unwrap()
        .value;
    invalid["default_model_slug"] = "nonexistent".into();
    assert!(codex2api_storage::validate_virtual_config("models", &invalid).is_err());
    invalid["default_model_slug"] = "example-model".into();
    invalid["versions"][0]["intelligence_presets"][0]["model_slug"] = "nonexistent".into();
    assert!(codex2api_storage::validate_virtual_config("models", &invalid).is_err());
    let mut other = account.clone();
    other.id = "second-admin-client".into();
    other.username = "second-admin-client".into();
    storage.save_virtual_account(&other).await.unwrap();
    assert!(
        storage
            .virtual_config(&other.id, "models")
            .await
            .unwrap()
            .value["models"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let client_state = storage
        .virtual_config(&account.id, "cloud_preferences")
        .await
        .unwrap();
    assert!(
        storage
            .update_virtual_admin_config(
                &account.id,
                "cloud_preferences",
                &client_state.value,
                client_state.revision
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn controls_policy_and_family_reads_match_actual_desktop_and_admin_ownership() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("controls-family.sqlite");
    let storage = Storage::open(&db).await.unwrap();
    let (api, account) = fixture(&storage).await;
    let app = api.merge(codex2api_admin::router(
        codex2api_admin::AdminState::new(storage.clone()).unwrap(),
    ));
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let family = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/amphora",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(family, json!({"id":null,"role":null}));
    let enabled = json_body(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/statsig/bootstrap",
                &token,
                json!({"stable_id":"controls-test"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let parsed: Value = serde_json::from_str(enabled["statsigPayload"].as_str().unwrap()).unwrap();
    for gate in ["410065390", "1506311413", "3693343337", "1186680773"] {
        assert_eq!(
            parsed["feature_gates"][codex2api_storage::statsig_hash(gate)]["value"],
            true
        );
    }
    let (cookie, csrf) = admin_login(&app).await;
    admin_save_config(
        &app,
        &account.id,
        "computer_use_policy",
        &cookie,
        &csrf,
        json!({"browser_enabled":false,"computer_enabled":false}),
    )
    .await;
    admin_save_config(
        &app,
        &account.id,
        "desktop_model_policy",
        &cookie,
        &csrf,
        json!({"reasoning_settings_enabled":false,"ultra_effort_available":false}),
    )
    .await;
    let disabled = json_body(
        app.clone()
            .oneshot(client_json(
                "POST",
                "/backend-api/wham/statsig/bootstrap",
                &token,
                json!({"stable_id":"controls-test"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let settings = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                &format!("/backend-api/accounts/{}/settings", account.id),
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(settings["beta_settings"]["windows_computer_use"], false);
    let current = storage.virtual_config(&account.id, "family").await.unwrap();
    let data = json!({"id":"family-a","role":"parent","members":[{"user_id":format!("user-{}",account.id),"role":"parent","status":"accepted","name":"Local parent"},{"user_id":"local-child","role":"child","status":"accepted","name":"Local child"}]});
    storage
        .update_virtual_config(&account.id, "family", &data, current.revision)
        .await
        .unwrap()
        .unwrap();
    let populated = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/amphora",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    let members = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/amphora/family-a/members",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(members["total"], 2);
    assert_eq!(members["items"][1]["name"], "Local child");
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/amphora/foreign-family/members",
                &token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let mut other = account.clone();
    other.id = "controls-other".into();
    other.username = "controls-other".into();
    storage.save_virtual_account(&other).await.unwrap();
    assert_eq!(
        storage
            .virtual_config(&other.id, "computer_use_policy")
            .await
            .unwrap()
            .value["browser_enabled"],
        true
    );
    assert!(
        storage
            .virtual_config(&other.id, "family")
            .await
            .unwrap()
            .value
            .is_null()
    );
    let view = admin_config(&app, &account.id, "family", &cookie).await;
    assert_eq!(view["readonly"], true);
    assert!(view["value"].to_string().contains("Local child"));
    if let Ok(archive) = std::env::var("CODEX2API_TEST_DESKTOP_ASAR") {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let mut child = Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/windows/Test-DesktopControlsAndFamily.cjs"),
            )
            .arg(archive)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(json!({"enabled":enabled,"disabled":disabled,"family":family,"populated_family":populated}).to_string().as_bytes()).unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    storage.close().await;
    let reopened = Storage::open(&db).await.unwrap();
    assert_eq!(
        reopened
            .virtual_config(&account.id, "computer_use_policy")
            .await
            .unwrap()
            .write_origin,
        "admin"
    );
    assert_eq!(
        reopened
            .virtual_config(&account.id, "family")
            .await
            .unwrap()
            .write_origin,
        "system"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ultra_slider_default_settings_parse_and_real_client_toggle_roundtrips() {
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ultra-slider.sqlite");
    let storage = Storage::open(&db).await.unwrap();
    let (api, account) = fixture(&storage).await;
    let app = api.merge(codex2api_admin::router(
        codex2api_admin::AdminState::new(storage.clone()).unwrap(),
    ));
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let settings = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/settings/user",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        !settings["settings"]
            .as_object()
            .unwrap()
            .contains_key("voice_name")
    );
    let existing = storage
        .virtual_config(&account.id, "user_settings")
        .await
        .unwrap();
    let mut preferences = existing.value;
    preferences["settings"]["chat_theme"] = json!("blue");
    storage
        .update_virtual_client_config(
            &account.id,
            "user_settings",
            &preferences,
            existing.revision,
        )
        .await
        .unwrap();
    let current = storage
        .virtual_config(&account.id, "user_settings")
        .await
        .unwrap();
    let mut policy = current.value;
    policy["flags"]["bazaar_consent_required"] = json!(true);
    storage
        .update_virtual_admin_config(&account.id, "user_settings", &policy, current.revision)
        .await
        .unwrap();
    if let Ok(source) = std::env::var("CODEX2API_TEST_DESKTOP_SOURCE") {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cloned = app.clone();
        let server = tokio::spawn(async move { axum::serve(listener, cloned).await.unwrap() });
        let data = json!({"base_url":format!("http://{addr}{ROOT}/backend-api"),"token":token,"account_id":account.id,"user_id":format!("user-{}",account.id)});
        let result = tokio::task::spawn_blocking(move || {
            use std::{
                io::Write,
                process::{Command, Stdio},
            };
            let mut child = Command::new("node")
                .arg(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../scripts/windows/Test-DesktopUltraSlider.mjs"),
                )
                .arg(source)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(data.to_string().as_bytes())
                .unwrap();
            child.wait_with_output().unwrap()
        })
        .await
        .unwrap();
        server.abort();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        println!("{}", String::from_utf8_lossy(&result.stdout));
    } else {
        for enabled in [true, false, true] {
            assert_eq!(app.clone().oneshot(client_json("PATCH",&format!("/backend-api/settings/account_user_setting?feature=model_picker_persists_ultra_effort&value={enabled}"),&token,Value::Null)).await.unwrap().status(),StatusCode::OK);
        }
    }
    let saved = storage
        .virtual_config(&account.id, "user_settings")
        .await
        .unwrap();
    assert_eq!(
        saved.value["settings"]["model_picker_persists_ultra_effort"],
        true
    );
    assert_eq!(saved.value["settings"]["chat_theme"], "blue");
    assert_eq!(saved.value["flags"]["bazaar_consent_required"], true);
    assert_eq!(saved.write_origin, "client");
    let mut other = account.clone();
    other.id = "slider-other".into();
    other.username = "slider-other".into();
    storage.save_virtual_account(&other).await.unwrap();
    assert!(
        storage
            .virtual_config(&other.id, "user_settings")
            .await
            .unwrap()
            .value["settings"]
            .get("model_picker_persists_ultra_effort")
            .is_none()
    );
    assert!(storage.update_virtual_admin_config(&account.id,"user_settings",&json!({"settings":{"model_picker_persists_ultra_effort":false},"flags":saved.value["flags"]}),saved.revision).await.is_err());
    for route in [
        "/backend-api/models",
        "/backend-api/tpp/models/",
        "/backend-api/tpp/models",
    ] {
        assert_eq!(
            app.clone()
                .oneshot(client_json("GET", route, &token, Value::Null))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "PATCH",
                "/backend-api/settings/account_user_setting?feature=voice_name&value=marin",
                &token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let value = json_body(
        app.clone()
            .oneshot(client_json(
                "GET",
                "/backend-api/settings/user",
                &token,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(value["settings"]["voice_name"], "marin");
    assert_eq!(
        value["settings"]["model_picker_persists_ultra_effort"],
        true
    );
    let (cookie, _csrf) = admin_login(&app).await;
    let view = json_body(
        app.clone()
            .oneshot(admin_request(
                "GET",
                &format!(
                    "/admin/api/consumers/{}/client-state/user_settings",
                    account.id
                ),
                &cookie,
                "",
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        view["value"]["settings"]["model_picker_persists_ultra_effort"],
        true
    );
    storage.close().await;
    let reopened = Storage::open(&db).await.unwrap();
    assert_eq!(
        reopened
            .virtual_config(&account.id, "user_settings")
            .await
            .unwrap()
            .value["settings"]["model_picker_persists_ultra_effort"],
        true
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn family_graduation_notices_are_owned_persistent_and_dismiss_only_captured_ids() {
    use codex2api_storage::FamilyNoticeRecipient::{Parent, Teen};
    use serde_json::json;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("graduation.sqlite");
    let storage = Storage::open(&db).await.unwrap();
    let (api, account) = fixture(&storage).await;
    let app = api.merge(codex2api_admin::router(
        codex2api_admin::AdminState::new(storage.clone()).unwrap(),
    ));
    let token = login(&app).await["access_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let base = "/backend-api/amphora/u18_graduation_unlink_setting_notices";
    assert_eq!(
        app.clone()
            .oneshot(client_json("GET", base, "wrong-token", Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let empty = app
        .clone()
        .oneshot(client_json("GET", base, &token, Value::Null))
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::OK);
    assert_eq!(json_body(empty).await, json!({"notices":[]}));
    assert!(
        storage
            .virtual_resource_records(&account.id, "family_graduation_notice")
            .await
            .unwrap()
            .is_empty()
    );
    let mut other = account.clone();
    other.id = "family-notice-other".into();
    other.username = "family-notice-other".into();
    storage.save_virtual_account(&other).await.unwrap();
    let foreign = storage
        .record_family_graduation_notice(&other.id, "event-foreign", Parent, "Foreign member", None)
        .await
        .unwrap();
    let first = storage
        .record_family_graduation_notice(&account.id, "event-one", Parent, "Member One", None)
        .await
        .unwrap();
    let second = storage
        .record_family_graduation_notice(&account.id, "event-two", Teen, "Member Two", None)
        .await
        .unwrap();
    assert_eq!(
        storage
            .record_family_graduation_notice(&account.id, "event-one", Parent, "Member One", None)
            .await
            .unwrap(),
        first
    );
    let captured = json_body(
        app.clone()
            .oneshot(client_json("GET", base, &token, Value::Null))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(captured["notices"].as_array().unwrap().len(), 2);
    assert!(!captured.to_string().contains("Foreign member"));
    assert!(!captured.to_string().contains("source_event_id"));
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "GET",
                &format!("{base}?account_id={}", other.id),
                &token,
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let dismiss = format!("{base}/dismiss");
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "POST",
                &dismiss,
                &token,
                json!({"notice_ids":[first,foreign]})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        storage
            .family_graduation_notices(&account.id, false)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "POST",
                &dismiss,
                &token,
                json!({"notice_ids":[]})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let newer = storage
        .record_family_graduation_notice(
            &account.id,
            "event-newer",
            Parent,
            "Newer Member",
            Some("https://example.test/local-family-policy"),
        )
        .await
        .unwrap();
    if let Ok(source) = std::env::var("CODEX2API_TEST_DESKTOP_SOURCE") {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cloned = app.clone();
        let server = tokio::spawn(async move { axum::serve(listener, cloned).await.unwrap() });
        let data = json!({"base_url":format!("http://{addr}{ROOT}/backend-api"),"token":token,"account_id":account.id,"user_id":format!("user-{}",account.id),"captured":captured,"newer_id":newer});
        let result = tokio::task::spawn_blocking(move || {
            use std::{
                io::Write,
                process::{Command, Stdio},
            };
            let mut child = Command::new("node")
                .arg(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../scripts/windows/Test-DesktopFamilyNotices.mjs"),
                )
                .arg(source)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(data.to_string().as_bytes())
                .unwrap();
            child.wait_with_output().unwrap()
        })
        .await
        .unwrap();
        server.abort();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        println!("{}", String::from_utf8_lossy(&result.stdout));
    } else {
        assert_eq!(
            app.clone()
                .oneshot(client_json(
                    "POST",
                    &dismiss,
                    &token,
                    json!({"notice_ids":[first]})
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    let closed_at = storage
        .family_graduation_notices(&account.id, true)
        .await
        .unwrap()
        .into_iter()
        .find(|n| n.id == first)
        .unwrap()
        .dismissed_at_ms;
    assert!(closed_at.is_some());
    assert_eq!(
        app.clone()
            .oneshot(client_json(
                "POST",
                &dismiss,
                &token,
                json!({"notice_ids":[first,first]})
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        storage
            .family_graduation_notices(&account.id, true)
            .await
            .unwrap()
            .into_iter()
            .find(|n| n.id == first)
            .unwrap()
            .dismissed_at_ms,
        closed_at
    );
    assert_eq!(
        storage
            .record_family_graduation_notice(&account.id, "event-one", Parent, "Member One", None)
            .await
            .unwrap(),
        first
    );
    let remaining = storage
        .family_graduation_notices(&account.id, false)
        .await
        .unwrap();
    assert_eq!(remaining.len(), 2);
    assert!(remaining.iter().any(|n| n.id == second));
    assert!(remaining.iter().any(|n| n.id == newer));
    assert_eq!(
        storage
            .family_graduation_notices(&other.id, false)
            .await
            .unwrap()
            .len(),
        1
    );
    let (cookie, csrf) = admin_login(&app).await;
    let view = json_body(
        app.clone()
            .oneshot(admin_request(
                "GET",
                &format!(
                    "/admin/api/consumers/{}/records?kind=family_notices",
                    account.id
                ),
                &cookie,
                &csrf,
                Value::Null,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(view.to_string().contains("Member One"));
    assert!(view.to_string().contains("Newer Member"));
    assert!(!view.to_string().contains("Foreign member"));
    assert!(
        view["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|notice| notice["dismissed_at_ms"].is_number())
    );
    assert!(
        storage
            .update_virtual_admin_config(&account.id, "family_notices", &json!({"notices":[]}), 0)
            .await
            .is_err()
    );
    assert!(
        !storage
            .missing_endpoints()
            .await
            .unwrap()
            .iter()
            .any(|m| m.path.contains("u18_graduation"))
    );
    storage.close().await;
    let reopened = Storage::open(&db).await.unwrap();
    assert_eq!(
        reopened
            .family_graduation_notices(&account.id, false)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        reopened
            .family_graduation_notices(&account.id, true)
            .await
            .unwrap()
            .len(),
        3
    );
}

// Change catalog entries in subscription fixtures through the same storage API as admin.
async fn set_test_plan_config(
    storage: &Storage,
    owner: &str,
    key: &str,
    value: &Value,
    revision: i64,
) -> codex2api_storage::Result<Option<i64>> {
    let account = storage.virtual_account(owner).await?.unwrap();
    let mut current = storage.virtual_plan(&account.plan_id).await?.unwrap();
    if current.revision != revision {
        return Ok(None);
    }
    if key == "quota" {
        for key in ["primary_cost_limit_usd", "weekly_cost_limit_usd"] {
            current.config[key] = value[key].clone();
        }
        if !storage.save_virtual_plan(&current, Some(revision)).await? {
            return Ok(None);
        }
    } else {
        for (kind, config) in value.as_object().unwrap() {
            let id = if kind == &account.plan_type {
                &account.plan_id
            } else {
                kind
            };
            let mut plan = storage.virtual_plan(id).await?.unwrap();
            for key in ["models", "primary_cost_limit_usd", "weekly_cost_limit_usd"] {
                if let Some(value) = config.get(key) {
                    plan.config[key] = value.clone();
                }
            }
            if let Some(models) = plan.config["models"].as_array_mut() {
                for model in models.iter_mut() {
                    if let Some(name) = model.as_str() {
                        *model = serde_json::json!({"provider_id":"chatgpt","model":name});
                    }
                }
            }
            plan.config["model_access"] = if plan.config["models"].as_array().unwrap().is_empty() {
                "all"
            } else {
                "selected"
            }
            .into();
            for model in plan.config["models"].as_array().unwrap() {
                sqlx::query("INSERT OR IGNORE INTO model_catalog(provider_id,model,kind) VALUES('chatgpt',?,'text')").bind(model["model"].as_str().unwrap()).execute(storage.pool()).await?;
            }
            assert!(
                storage
                    .save_virtual_plan(&plan, Some(plan.revision))
                    .await?
            );
        }
    }
    Ok(Some(
        storage
            .virtual_plan(&account.plan_id)
            .await?
            .unwrap()
            .revision,
    ))
}

async fn test_supplier(storage: &Storage, account: &VirtualAccount) -> Option<String> {
    storage
        .execution_route(&account.id, "chatgpt")
        .await
        .unwrap()
        .and_then(|r| r.supplier_account_id)
}
async fn bind_test_supplier(storage: &Storage, account: &VirtualAccount, supplier: Option<String>) {
    let previous = storage
        .execution_route(&account.id, "chatgpt")
        .await
        .unwrap();
    assert!(
        storage
            .save_execution_route(
                &account.id,
                "chatgpt",
                supplier.as_deref(),
                previous.map(|r| r.revision)
            )
            .await
            .unwrap()
    );
}

mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::{Value, json};
#[tokio::test]
async fn consumers_are_provider_fixed_and_never_expose_passwords() {
    let f = Fixture::new().await;
    let c = f.consumer("alice").await;
    let id = c["id"].as_str().unwrap();
    assert!(c.get("password_hash").is_none());
    assert!(c.get("password").is_none());
    assert!(
        !f.get("/admin/api/consumers")
            .await
            .to_string()
            .contains("secret-fixture")
    );
    let mut input = json!({"username":"alice","password":"","provider_id":"grok","name":"Alice","email":"alice@example.test","plan_id":"plus","subscription_expires_at":null,"enabled":true});
    let path = format!("/admin/api/consumers/{id}");
    assert_eq!(
        f.request("PUT", &path, input.clone()).await.status(),
        StatusCode::BAD_REQUEST
    );
    input["provider_id"] = "chatgpt".into();
    let previous = f
        .storage
        .virtual_account(id)
        .await
        .unwrap()
        .unwrap()
        .password_hash;
    assert_eq!(
        f.request("PUT", &path, input.clone()).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        f.storage
            .virtual_account(id)
            .await
            .unwrap()
            .unwrap()
            .password_hash,
        previous
    );
    input["account_id"] = "legacy-supplier".into();
    assert_eq!(
        f.request("PUT", &path, input).await.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
#[tokio::test]
async fn plans_are_shared_revision_checked_and_subscription_operations_are_actual() {
    let f = Fixture::new().await;
    let input = plan_input();
    let r = f.request("POST", "/admin/api/plans", input.clone()).await;
    assert_eq!(r.status(), StatusCode::OK);
    let plan = body(r).await;
    let id = plan["id"].as_str().unwrap();
    let path = format!("/admin/api/plans/{id}");
    let mut changed = input;
    changed["revision"] = plan["revision"].clone();
    changed["model_access"] = "selected".into();
    assert_eq!(
        f.request("PUT", &path, changed.clone()).await.status(),
        StatusCode::BAD_REQUEST
    );
    changed["model_access"] = "all".into();
    changed["name"] = "Renamed".into();
    assert_eq!(
        f.request("PUT", &path, changed.clone()).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        f.request("PUT", &path, changed).await.status(),
        StatusCode::CONFLICT
    );
    let c = f.consumer("subscriber").await;
    let owner = c["id"].as_str().unwrap();
    let input = json!({"username":"subscriber","password":"","provider_id":"chatgpt","name":"subscriber","email":"subscriber@example.test","plan_id":id,"subscription_expires_at":"2020-01-01T00:00:00Z","enabled":true});
    let r = f
        .request("PUT", &format!("/admin/api/consumers/{owner}"), input)
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(body(r).await["effective_plan"], "free");
    let records = f
        .get(&format!(
            "/admin/api/consumers/{owner}/records?kind=subscription_operation"
        ))
        .await;
    assert!(!records["items"].as_array().unwrap().is_empty());
    assert_eq!(
        f.request(
            "DELETE",
            &path,
            json!({"revision":plan["revision"].as_i64().unwrap()+1})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn service_configuration_validates_revisions_and_preserves_client_owned_state() {
    let f = Fixture::new().await;
    let c = f.consumer("managed").await;
    let id = c["id"].as_str().unwrap();
    let base = format!("/admin/api/consumers/{id}");
    let list = f.get(&format!("{base}/configs")).await;
    assert!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["key"] == "account_settings" && v["fields"].is_array())
    );
    for key in [
        "projects",
        "pins",
        "automations",
        "onboarding",
        "browser_settings",
        "voice",
        "installed_plugins",
        "notifications",
        "notification_settings",
        "referral_tracking",
        "payment_methods",
        "auto_top_up",
        "config_bundle",
        "models",
        "quota",
        "subscription_policy",
        "subscription_entitlements",
    ] {
        let before = f
            .storage
            .virtual_client_state(id, key)
            .await
            .unwrap()
            .map(|v| v.revision);
        assert_eq!(
            f.request(
                "PUT",
                &format!("{base}/config/{key}"),
                json!({"revision":0,"value":{}})
            )
            .await
            .status(),
            StatusCode::FORBIDDEN,
            "{key}"
        );
        assert_eq!(
            f.storage
                .virtual_client_state(id, key)
                .await
                .unwrap()
                .map(|v| v.revision),
            before
        );
    }
    let path = format!("{base}/config/workspace_messages");
    let current = f
        .storage
        .virtual_config(id, "workspace_messages")
        .await
        .unwrap();
    let value = json!({"revision":current.revision,"value":{"messages":[{"message_type":"info","message_body":"<script>inert</script>"}]}});
    let r = f.request("PUT", &path, value.clone()).await;
    assert_eq!(r.status(), StatusCode::OK);
    let saved = body(r).await;
    assert!(saved["value"]["messages"][0]["message_id"].is_string());
    assert_eq!(
        f.request("PUT", &path, value).await.status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        f.request(
            "PUT",
            &path,
            json!({"revision":saved["revision"],"value":{"messages":false}})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let current = f.storage.virtual_config(id, "user_settings").await.unwrap();
    let path = format!("{base}/config/user_settings");
    let mut changed = current.value.clone();
    changed["settings"] = json!({"chat_theme":"blue"});
    assert_eq!(
        f.request(
            "PUT",
            &path,
            json!({"revision":current.revision,"value":changed})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let mut allowed = current.value;
    allowed["flags"]["bazaar_consent_required"] = true.into();
    assert_eq!(
        f.request(
            "PUT",
            &path,
            json!({"revision":current.revision,"value":allowed})
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        f.storage
            .virtual_config(id, "user_settings")
            .await
            .unwrap()
            .write_origin,
        "admin"
    );
}
#[tokio::test]
async fn consumer_history_and_quota_are_isolated_and_only_have_two_cost_windows() {
    let f = Fixture::new().await;
    let a = f.consumer("a").await;
    let b = f.consumer("b").await;
    let a = a["id"].as_str().unwrap();
    let b = b["id"].as_str().unwrap();
    let mut plan = f.storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["primary_cost_limit_usd"] = 2.into();
    plan.config["weekly_cost_limit_usd"] = 10.into();
    assert!(
        f.storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap()
    );
    let mut record = codex2api_storage::UsageRecord {
        id: "a-record".into(),
        subject_id: a.into(),
        subject_name: "a".into(),
        account_id: "supplier".into(),
        input_tokens: Some(100),
        output_tokens: Some(20),
        status: "in_progress".into(),
        requested_at_ms: chrono::Utc::now().timestamp_millis(),
        ..Default::default()
    };
    f.storage.insert_usage(&record).await.unwrap();
    record.status = "completed".into();
    f.storage.finish_usage(&record).await.unwrap();
    let usage = f.get(&format!("/admin/api/consumers/{a}/usage")).await;
    assert_eq!(usage["summary"]["lifetime_tokens"], 120);
    let other = f.get(&format!("/admin/api/consumers/{b}/usage")).await;
    assert_ne!(other["summary"]["lifetime_tokens"], 120);
    assert_eq!(
        usage["quota"]["rate_limit"]["primary_window"]["limit_usd"],
        json!("2")
    );
    assert_eq!(
        usage["quota"]["rate_limit"]["secondary_window"]["limit_usd"],
        json!("10")
    );
    assert!(usage["quota"]["billing"].get("used_usd").is_some());
    assert!(!usage.to_string().contains("total_cost_limit"));
    assert!(f.get(&format!("/admin/api/consumers/{a}/devices")).await["remote_servers"].is_array());
    assert_eq!(
        f.request("GET", "/admin/api/consumers/missing/configs", Value::Null)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn configuration_options_use_separate_account_owned_lists() {
    let f = Fixture::new().await;
    let first = f.consumer("option-owner").await;
    let second = f.consumer("option-other").await;
    let owner = first["id"].as_str().unwrap();
    let other = second["id"].as_str().unwrap();
    // Seed a persisted client observation without granting a new client write capability.
    sqlx::query("INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision,write_origin,updated_at_ms) VALUES(?,'installed_plugins',?,1,'client',1)")
        .bind(owner).bind(json!({"plugins":[{"id":"owner-plugin","name":"Owner plugin"}]}).to_string())
        .execute(f.storage.pool()).await.unwrap();
    f.storage
        .save_virtual_resource(
            owner,
            "connector_catalog",
            "owner-connector",
            None,
            &json!({"id":"owner-connector","name":"Owner connector"}),
        )
        .await
        .unwrap();
    assert_eq!(
        f.get(&format!("/admin/api/consumers/{owner}/plugins"))
            .await["items"][0]["id"],
        "owner-plugin"
    );
    assert_eq!(
        f.get(&format!("/admin/api/consumers/{owner}/connectors"))
            .await["items"][0]["id"],
        "owner-connector"
    );
    for kind in ["plugins", "connectors"] {
        assert!(
            f.get(&format!("/admin/api/consumers/{other}/{kind}")).await["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            f.request(
                "GET",
                &format!("/admin/api/consumers/missing/{kind}"),
                json!(null)
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            f.with_auth(
                "GET",
                &format!("/admin/api/consumers/{owner}/{kind}"),
                json!(null),
                None,
                None
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }
}

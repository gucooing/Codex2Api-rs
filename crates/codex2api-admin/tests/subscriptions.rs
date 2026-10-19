mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
use account_fixture::AccountFixture;
mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::{Value, json};

#[tokio::test]
async fn realtime_creation_records_are_readonly_and_scoped_to_the_consumer() {
    let f = Fixture::new().await;
    let owner = f.consumer("voice-owner").await;
    let other = f.consumer("voice-other").await;
    let id = owner["id"].as_str().unwrap();
    let call = json!({"id":"rtc_fixture","model":"gpt-live-1-codex","transcription_model":null,"status":"created","created_at_ms":1234});
    f.storage
        .save_virtual_resource(id, "realtime_call", "rtc_fixture", None, &call)
        .await
        .unwrap();
    let path = format!("/admin/api/subscriptions/{id}/records?kind=realtime_call");
    let response = f.get(&path).await;
    assert_eq!(response["items"].as_array().unwrap().len(), 1);
    assert_eq!(response["items"][0]["value"], call);
    let empty = f
        .get(&format!(
            "/admin/api/subscriptions/{}/records?kind=realtime_call",
            other["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(empty["items"], json!([]));
    assert_eq!(
        f.with_auth("GET", &path, Value::Null, None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(!f.request("POST", &path, call).await.status().is_success());
}

#[tokio::test]
async fn subscriptions_keep_the_owner_and_provider_fixed_and_never_expose_passwords() {
    let f = Fixture::new().await;
    let account = f.consumer("alice").await;
    let id = account["id"].as_str().unwrap();
    assert!(account.get("password_hash").is_none());
    let mut input = json!({"user_id":account["user_id"],"plan_id":"plus","expires_at":null,"enabled":true,"reissue":false,"revision":account["revision"]});
    let path = format!("/admin/api/subscriptions/{id}");
    input["user_id"] = "someone-else".into();
    assert_eq!(
        f.request("PUT", &path, input.clone()).await.status(),
        StatusCode::NOT_FOUND
    );
    input["user_id"] = account["user_id"].clone();
    let grok = f.storage.plan_fixture("grok").await.unwrap();
    input["plan_id"] = grok.id.into();
    assert_eq!(
        f.request("PUT", &path, input.clone()).await.status(),
        StatusCode::CONFLICT
    );
    input["plan_id"] = "plus".into();
    assert_eq!(
        f.request("PUT", &path, input.clone()).await.status(),
        StatusCode::OK
    );
    input["password"] = "not-a-subscription-field".into();
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
    changed["models"] = serde_json::json!([
        {"model":"bad model","multiplier":null,"concurrency_limit":0,"rpm_limit":0}
    ]);
    assert_eq!(
        f.request("PUT", &path, changed.clone()).await.status(),
        StatusCode::BAD_REQUEST
    );
    changed["models"] = plan["models"].clone();
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
    let input = json!({"user_id":c["user_id"],"plan_id":id,"expires_at":"2020-01-01T00:00:00Z","enabled":true,"reissue":false,"revision":c["revision"]});
    let r = f
        .request("PUT", &format!("/admin/api/subscriptions/{owner}"), input)
        .await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(body(r).await["subscription_status"], "expired");
    let records = f
        .get(&format!(
            "/admin/api/subscriptions/{owner}/records?kind=subscription_operation"
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
    let base = format!("/admin/api/subscriptions/{id}");
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
        "installed_plugins",
        "notifications",
        "notification_settings",
        "referral_tracking",
        "payment_methods",
        "auto_top_up",
        "config_bundle",
        "models",
        "quota",
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
    plan.config["spending_windows"] = serde_json::json!([{"duration_seconds":604800,"cost_limit_usd":"10"},{"duration_seconds":18000,"cost_limit_usd":"2"}]);
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
    let usage = f.get(&format!("/admin/api/subscriptions/{a}/usage")).await;
    assert_eq!(usage["summary"]["lifetime_tokens"], 120);
    let other = f.get(&format!("/admin/api/subscriptions/{b}/usage")).await;
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
    assert!(
        f.get(&format!("/admin/api/subscriptions/{a}/remote-servers"))
            .await["items"]
            .is_array()
    );
    assert_eq!(
        f.request(
            "GET",
            "/admin/api/subscriptions/missing/configs",
            Value::Null
        )
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
    sqlx::query("INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision,write_origin,updated_at_ms) VALUES($1,'installed_plugins',$2::jsonb,1,'client',1)")
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
        f.get(&format!("/admin/api/subscriptions/{owner}/plugins"))
            .await["items"][0]["id"],
        "owner-plugin"
    );
    assert_eq!(
        f.get(&format!("/admin/api/subscriptions/{owner}/connectors"))
            .await["items"][0]["id"],
        "owner-connector"
    );
    for kind in ["plugins", "connectors"] {
        assert!(
            f.get(&format!("/admin/api/subscriptions/{other}/{kind}"))
                .await["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            f.request(
                "GET",
                &format!("/admin/api/subscriptions/missing/{kind}"),
                json!(null)
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            f.with_auth(
                "GET",
                &format!("/admin/api/subscriptions/{owner}/{kind}"),
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

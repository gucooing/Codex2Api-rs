mod common;
use common::*;
use serde_json::{Value, json};
fn normalize(v: &mut Value, key: &str) {
    match v {
        Value::Object(map) => {
            for (k, v) in map {
                normalize(v, k)
            }
        }
        Value::Array(items) => {
            for v in items {
                normalize(v, key)
            }
        }
        Value::String(s) if key == "csrf_token" => *s = "session-csrf-token".into(),
        Value::String(s)
            if matches!(
                key,
                "created_at" | "updated_at" | "last_used_at" | "last_login_at"
            ) =>
        {
            *s = "2026-09-22T00:00:00+00:00".into()
        }
        Value::Number(_)
            if key.ends_with("_ms") || key == "reset_at" || key == "reset_after_seconds" =>
        {
            *v = json!(1)
        }
        _ => {}
    }
}
fn shape(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), shape(v))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(shape).collect()),
        Value::Null => Value::Null,
        Value::Bool(_) => json!("boolean"),
        Value::Number(_) => json!("number"),
        Value::String(_) => json!("string"),
    }
}
#[tokio::test]
async fn real_rest_contracts_match_frontend_fixture() {
    let f = Fixture::new().await;
    let supplier = f.state.accounts.create_pending().await.unwrap().account;
    let consumer = codex2api_storage::VirtualAccount {
        id: "contract-consumer".into(),
        provider_id: "chatgpt".into(),
        username: "contract-consumer".into(),
        password_hash: "not-a-real-secret".into(),
        name: "Contract consumer".into(),
        email: "consumer@example.test".into(),
        plan_id: "plus".into(),
        plan_type: "plus".into(),
        subscription_expires_at: None,
        enabled: true,
        created_at: "2026-09-22T00:00:00Z".into(),
    };
    f.storage.save_virtual_account(&consumer).await.unwrap();
    let mut plan = f.storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["primary_cost_limit_usd"] = 2.into();
    plan.config["weekly_cost_limit_usd"] = 10.into();
    assert!(
        f.storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap()
    );
    f.storage
        .save_execution_route(&consumer.id, "chatgpt", None, None)
        .await
        .unwrap();
    let mut record = codex2api_storage::UsageRecord {
        id: "contract-usage".into(),
        account_id: "contract-supplier".into(),
        account_name: "Contract supplier".into(),
        subject_id: consumer.id.clone(),
        subject_name: consumer.name.clone(),
        provider_id: "chatgpt".into(),
        model: Some("gpt-6-astra".into()),
        transport: "http".into(),
        endpoint: "/v1/responses".into(),
        requested_at_ms: chrono::Utc::now().timestamp_millis(),
        status: "in_progress".into(),
        ..Default::default()
    };
    f.storage.insert_usage(&record).await.unwrap();
    record.input_tokens = Some(1000);
    record.output_tokens = Some(100);
    record.cached_tokens = Some(100);
    record.cache_write_tokens = Some(0);
    record.reasoning_tokens = Some(20);
    record.http_status = Some(200);
    record.status = "completed".into();
    f.storage.finish_usage(&record).await.unwrap();
    f.storage.save_virtual_resource(&consumer.id,"task","contract-task",Some(&supplier.id),&json!({"task":{"id":"contract-task","title":"Recorded test task","status":"completed"}})).await.unwrap();
    f.storage
        .save_virtual_resource(
            &consumer.id,
            "task_execution",
            "contract-task",
            Some(&supplier.id),
            &json!({"task_id":"contract-task","model":"gpt-6-astra"}),
        )
        .await
        .unwrap();
    f.storage.save_virtual_resource(&consumer.id,"task_operation","operation-fixture",Some(&supplier.id),&json!({"task_id":"contract-task","operation":"follow_up","status":"failed","reason":"task_model_unavailable","created_at_ms":1})).await.unwrap();
    for operation in ["grant", "renew", "change_plan", "change_expiry"] {
        f.storage.save_virtual_resource(&consumer.id,"subscription_operation",operation,None,&json!({"operation":operation,"previous_plan_name":"Previous plan","plan_name":"Plus","expires_at":"2027-09-22T00:00:00Z","origin":"admin","created_at_ms":1})).await.unwrap();
    }
    f.storage
        .record_desktop_diagnostic(
            "fixture-batch",
            "ces",
            Some(&consumer.id),
            1,
            &json!([{"type":"network","count":1}]),
        )
        .await
        .unwrap();
    f.storage
        .save_virtual_client_state(
            &consumer.id,
            "cloud_preferences",
            &json!({"branch_format":"codex/{task_id}","git_diff_mode":"unified"}),
            None,
        )
        .await
        .unwrap();
    let mut fixture = json!({"session":f.get("/admin/api/session").await,"overview":f.get("/admin/api/overview").await,"plans":f.get("/admin/api/plans").await,"models":f.get("/admin/api/models").await,"consumer":f.get("/admin/api/consumers/contract-consumer").await,"routing":f.get("/admin/api/consumers/contract-consumer/routing").await,"usage":f.get("/admin/api/usage").await,"task_records":f.get("/admin/api/consumers/contract-consumer/records?kind=task").await,"subscription_records":f.get("/admin/api/consumers/contract-consumer/records?kind=subscription_operation").await,"diagnostics":f.get("/admin/api/diagnostics").await,"client_state":f.get("/admin/api/consumers/contract-consumer/client-state/cloud_preferences").await,"consumer_usage":f.get("/admin/api/consumers/contract-consumer/usage").await});
    normalize(&mut fixture, "");
    fixture["task_execution_records"] = f
        .get("/admin/api/consumers/contract-consumer/records?kind=task_execution")
        .await;
    fixture["task_operation_records"] = f
        .get("/admin/api/consumers/contract-consumer/records?kind=task_operation")
        .await;
    normalize(&mut fixture, "");
    assert!(fixture["routing"].get("suppliers").is_none());
    assert!(fixture["plans"].get("model_choices").is_none());
    assert!(fixture["usage"].get("consumers").is_none());
    assert!(fixture["usage"].get("suppliers").is_none());
    assert!(
        f.get("/admin/api/suppliers/oauth/setup")
            .await
            .get("proxies")
            .is_none()
    );
    for config in f
        .get("/admin/api/consumers/contract-consumer/configs")
        .await["items"]
        .as_array()
        .unwrap()
    {
        assert!(config.get("choices").is_none());
    }
    assert!(fixture["consumer"].get("password_hash").is_none());
    assert!(fixture["consumer_usage"]["quota"]["billing"]["used_usd"].is_string());
    assert_ne!(
        fixture["consumer_usage"]["quota"]["billing"]["used_usd"],
        "0"
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/contracts.json");
    if std::env::var_os("CODEX2API_UPDATE_CONTRACT_FIXTURE").is_some() {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&fixture).unwrap() + "\n",
        )
        .unwrap();
    }
    let expected:Value=serde_json::from_slice(&std::fs::read(path).expect("Generate contracts.json with CODEX2API_UPDATE_CONTRACT_FIXTURE=1 cargo test -p codex2api-admin --test contracts")).unwrap();
    assert_eq!(
        shape(&fixture),
        shape(&expected),
        "REST contract changed: update frontend readers and explicitly regenerate fixture"
    );
}

use codex2api_core::{ExecutionKind, PolicyError};
use codex2api_service::{ExecutionRequest, ExecutionService, ServiceError};
use codex2api_storage::{Storage, VirtualAccount};
use serde_json::json;

fn request(model: &str) -> ExecutionRequest<'_> {
    ExecutionRequest {
        kind: ExecutionKind::Responses,
        model: Some(model),
        service_tier: None,
        image_size: None,
    }
}

async fn consumer(storage: &Storage, id: &str) -> VirtualAccount {
    let account = VirtualAccount {
        provider_id: "chatgpt".into(),
        id: id.into(),
        username: id.into(),
        password_hash: "fixture".into(),
        name: id.into(),
        email: format!("{id}@example.test"),
        plan_type: "plus".into(),
        plan_id: "plus".into(),
        subscription_expires_at: None,
        enabled: true,
        created_at: chrono_timestamp(),
    };
    storage.save_virtual_account(&account).await.unwrap();
    account
}

fn chrono_timestamp() -> String {
    "2026-09-22T00:00:00Z".into()
}

#[tokio::test]
async fn model_permissions_are_exact_current_and_provider_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("policy.sqlite"))
        .await
        .unwrap();
    let account = consumer(&storage, "consumer").await;
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["model_access"] = json!("selected");
    plan.config["models"] = json!([{"provider_id":"chatgpt","model":"gpt-5.6-luna"}]);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let service = ExecutionService::new(storage.clone());
    service
        .authorize(&account.id, "chatgpt", request("gpt-5.6-luna"))
        .await
        .unwrap();
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", request("gpt-6-astra"))
            .await,
        Err(ServiceError::Policy(PolicyError::ModelNotEntitled))
    ));
    assert!(
        service
            .authorize(&account.id, "grok", request("gpt-5.6-luna"))
            .await
            .is_err()
    );
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", request(&"x".repeat(257)))
            .await,
        Err(ServiceError::Policy(PolicyError::InvalidModel))
    ));
    let model = storage
        .model_config("chatgpt", "gpt-5.6-luna")
        .await
        .unwrap()
        .unwrap();
    storage
        .set_model_enabled("chatgpt", &model.model, false, model.revision)
        .await
        .unwrap();
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", request(&model.model))
            .await,
        Err(ServiceError::Policy(PolicyError::ModelUnavailable))
    ));
    assert!(
        storage
            .available_virtual_models(&account.id, "chatgpt")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn expiry_uses_only_explicit_free_entitlements_and_never_expands_to_all() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("expiry.sqlite"))
        .await
        .unwrap();
    let mut account = consumer(&storage, "consumer").await;
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["model_access"] = json!("selected");
    plan.config["models"] = json!([{"provider_id":"chatgpt","model":"gpt-6-astra"}]);
    plan.config["free_access_enabled"] = json!(true);
    plan.config["free_primary_cost_limit_usd"] = json!(null);
    plan.config["free_weekly_cost_limit_usd"] = json!(null);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let service = ExecutionService::new(storage.clone());
    service
        .authorize(&account.id, "chatgpt", request("gpt-6-astra"))
        .await
        .unwrap();
    account.subscription_expires_at = Some("2020-01-01T00:00:00Z".into());
    storage.save_virtual_account(&account).await.unwrap();
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", request("gpt-6-astra"))
            .await,
        Err(ServiceError::Policy(PolicyError::ModelNotEntitled))
    ));
    assert!(
        storage
            .available_virtual_models(&account.id, "chatgpt")
            .await
            .unwrap()
            .is_empty()
    );
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["free_model_access"] = json!("selected");
    plan.config["free_models"] = json!([{"provider_id":"chatgpt","model":"gpt-5.6-luna"}]);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    service
        .authorize(&account.id, "chatgpt", request("gpt-5.6-luna"))
        .await
        .unwrap();
    assert!(
        service
            .authorize(&account.id, "chatgpt", request("gpt-6-astra"))
            .await
            .is_err()
    );
    assert!(
        storage
            .virtual_account(&account.id)
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
}

#[tokio::test]
async fn all_models_means_configured_and_enabled_and_consumer_disable_is_immediate() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("all.sqlite")).await.unwrap();
    let mut account = consumer(&storage, "consumer").await;
    let service = ExecutionService::new(storage.clone());
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", request("not-configured"))
            .await,
        Err(ServiceError::Policy(PolicyError::ModelUnavailable))
    ));
    service
        .authorize(&account.id, "chatgpt", request("gpt-6-astra"))
        .await
        .unwrap();
    account.enabled = false;
    storage.save_virtual_account(&account).await.unwrap();
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", request("gpt-6-astra"))
            .await,
        Err(ServiceError::Policy(PolicyError::SubscriptionRequired))
    ));
}

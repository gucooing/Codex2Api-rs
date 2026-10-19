#[cfg(test)]
mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
#[cfg(test)]
use account_fixture::AccountFixture;
use codex2api_common::{ExecutionKind, PolicyError};
use codex2api_oauth::admission::{ExecutionRequest, ExecutionService, ServiceError};
use codex2api_storage::{PlatformAccount, Storage};
use serde_json::json;

fn request(model: &str) -> ExecutionRequest<'_> {
    ExecutionRequest {
        kind: ExecutionKind::Responses,
        model: Some(model),
        service_tier: None,
        image_size: None,
    }
}

async fn consumer(storage: &Storage, id: &str) -> PlatformAccount {
    let account = PlatformAccount {
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
    storage.save_account_fixture(&account).await.unwrap();
    account
}

fn chrono_timestamp() -> String {
    "2026-09-22T00:00:00Z".into()
}

#[tokio::test]
async fn image_pricing_cache_preserves_size_matching_and_price_edits() {
    let temp = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(temp.path().join("image-prices-pg"))
        .await
        .unwrap();
    let account = consumer(&storage, "consumer").await;
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["model_access"] = json!("all");
    plan.config["spending_windows"] = json!([{"duration_seconds":604800,"cost_limit_usd":"1"}]);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let model = codex2api_storage::ModelConfig {
        platform: "chatgpt".into(),
        model: "fixture-image".into(),
        kind: "image".into(),
        enabled: true,
        deleted: false,
        revision: 0,
    };
    let mut price = codex2api_storage::ImagePrice {
        model: model.model.clone(),
        resolution: "4K".into(),
        price_nano_usd: 1,
    };
    storage
        .save_model_config(&model, &[], std::slice::from_ref(&price), None)
        .await
        .unwrap();
    let service = ExecutionService::new(storage.clone());
    let image = |size| ExecutionRequest {
        kind: ExecutionKind::Image,
        image_size: size,
        ..request("fixture-image")
    };
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", image(Some("1024x1024")))
            .await,
        Err(ServiceError::PricingUnavailable)
    ));
    let model = storage.model_config(&model.model).await.unwrap().unwrap();
    price.resolution = "1K".into();
    storage
        .save_model_config(
            &model,
            &[],
            std::slice::from_ref(&price),
            Some(model.revision),
        )
        .await
        .unwrap();
    for size in [None, Some("auto"), Some("1K"), Some("1024x1024")] {
        service
            .authorize(&account.id, "chatgpt", image(size))
            .await
            .unwrap();
    }
    for size in ["4K", "4096x4096", "unknown"] {
        assert!(matches!(
            service
                .authorize(&account.id, "chatgpt", image(Some(size)))
                .await,
            Err(ServiceError::PricingUnavailable)
        ));
    }
    let model = storage.model_config(&model.model).await.unwrap().unwrap();
    price.resolution = "4K".into();
    storage
        .save_model_config(
            &model,
            &[],
            std::slice::from_ref(&price),
            Some(model.revision),
        )
        .await
        .unwrap();
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", image(Some("1024x1024")))
            .await,
        Err(ServiceError::PricingUnavailable)
    ));
}

#[tokio::test]
async fn configured_search_price_allows_limited_accounts_without_bypassing_model_or_budget_checks()
{
    let temp = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(temp.path().join("search-pg"))
        .await
        .unwrap();
    let account = consumer(&storage, "consumer").await;
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["spending_windows"] = json!([{"duration_seconds":604800,"cost_limit_usd":"1"}]);
    plan.config["model_access"] = json!("selected");
    plan.config["models"] = json!([{"provider_id":"chatgpt","model":"gpt-6-astra"}]);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let service = ExecutionService::new(storage.clone());
    let search = || ExecutionRequest {
        kind: ExecutionKind::Operation("search"),
        ..request("gpt-6-astra")
    };
    assert!(matches!(
        service.authorize(&account.id, "chatgpt", search()).await,
        Err(ServiceError::PricingUnavailable)
    ));
    storage
        .save_operation_price(
            &codex2api_storage::OperationPrice {
                provider_id: "chatgpt".into(),
                operation: "search".into(),
                price_nano_usd: Some(100_000_000),
                revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    service
        .authorize(&account.id, "chatgpt", search())
        .await
        .unwrap();
    assert!(matches!(
        service
            .authorize(
                &account.id,
                "chatgpt",
                ExecutionRequest {
                    model: Some("gpt-5.6-luna"),
                    ..search()
                }
            )
            .await,
        Err(ServiceError::Policy(PolicyError::ModelNotEntitled))
    ));
    let mut record = codex2api_storage::UsageRecord {
        id: "search".into(),
        subject_id: account.id.clone(),
        endpoint: "/v1/alpha/search".into(),
        model: Some("gpt-6-astra".into()),
        requested_at_ms: chrono::Utc::now().timestamp_millis(),
        status: "in_progress".into(),
        ..Default::default()
    };
    for i in 0..10 {
        record.id = format!("search-{i}");
        record.status = "in_progress".into();
        storage.insert_usage(&record).await.unwrap();
        record.status = "completed".into();
        storage.finish_usage(&record).await.unwrap();
    }
    assert!(matches!(
        service.authorize(&account.id, "chatgpt", search()).await,
        Err(ServiceError::BudgetExceeded)
    ));
}

#[tokio::test]
async fn model_permissions_are_exact_current_and_provider_scoped() {
    let temp = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(temp.path().join("policy-pg"))
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
    let model = storage.model_config("gpt-5.6-luna").await.unwrap().unwrap();
    storage
        .set_model_enabled(&model.model, false, model.revision)
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
            .available_virtual_models(&account.id)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn expired_subscriptions_never_fall_back_to_another_plan() {
    let temp = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(temp.path().join("expiry-pg"))
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
    storage.save_account_fixture(&account).await.unwrap();
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", request("gpt-6-astra"))
            .await,
        Err(ServiceError::Policy(PolicyError::SubscriptionRequired))
    ));
    assert!(
        storage
            .available_virtual_models(&account.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(storage.virtual_plan("free").await.unwrap().is_none());
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", request("gpt-5.6-luna"))
            .await,
        Err(ServiceError::Policy(PolicyError::SubscriptionRequired))
    ));
    assert!(
        storage
            .platform_account(&account.id)
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
}

#[tokio::test]
async fn all_models_means_configured_and_enabled_and_consumer_disable_is_immediate() {
    let temp = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(temp.path().join("all-pg"))
        .await
        .unwrap();
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
    storage.save_account_fixture(&account).await.unwrap();
    assert!(matches!(
        service
            .authorize(&account.id, "chatgpt", request("gpt-6-astra"))
            .await,
        Err(ServiceError::Policy(PolicyError::SubscriptionRequired))
    ));
}

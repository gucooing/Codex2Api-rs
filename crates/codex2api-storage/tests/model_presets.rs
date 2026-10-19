use codex2api_common::SupportedModel;
use codex2api_storage::{UsageFilter, UsageRecord, preset_model_prices};
use serde_json::json;

fn supported(model: &str) -> SupportedModel {
    SupportedModel::builtin("chatgpt", model)
}

#[tokio::test]
async fn builtin_models_register_enabled_with_prices_once_and_restarts_are_idempotent() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let path = dir.path().join("models-pg");
    let storage = codex2api_storage::test_support::open(&path).await.unwrap();
    // Every platform's built-in model is already registered by the fixture bootstrap.
    for entry in codex2api_common::BUILTIN_MODELS {
        let config = storage.model_config(entry.model).await.unwrap().unwrap();
        assert!(config.enabled, "{}", entry.model);
        assert_eq!(config.platform, entry.platform, "{}", entry.model);
    }
    let prices = storage.model_prices().await.unwrap();
    assert!(
        prices
            .iter()
            .any(|p| p.model == "grok-4.6" && p.min_input_tokens == 200_000)
    );
    assert!(
        prices
            .iter()
            .any(|p| p.model == "claude-haiku-5.5" && p.min_input_tokens == 100_001)
    );
    let models = [supported("gpt-6.1-sol"), supported("future-supported")];
    let (a, b) = tokio::join!(
        storage.sync_supported_models(&models),
        storage.sync_supported_models(&models)
    );
    assert_eq!(a.unwrap() + b.unwrap(), 1);
    let pending = storage
        .model_config("future-supported")
        .await
        .unwrap()
        .unwrap();
    assert!(!pending.enabled);
    assert!(!prices.iter().any(|price| price.model == "future-supported"));
    let base = prices
        .iter()
        .find(|price| {
            price.model == "gpt-6.1-sol" && price.tier == "standard" && price.min_input_tokens == 0
        })
        .unwrap();
    assert_eq!(
        (
            base.input_rate,
            base.cached_rate,
            base.cache_write_rate,
            base.output_rate
        ),
        (2_000_000, 100_000, 2_500_000, 10_000_000)
    );
    let before = json!([
        storage.model_configs().await.unwrap(),
        storage.model_prices().await.unwrap()
    ]);
    storage.close().await;
    let reopened = codex2api_storage::test_support::open(&path).await.unwrap();
    assert_eq!(reopened.sync_supported_models(&models).await.unwrap(), 0);
    assert_eq!(
        json!([
            reopened.model_configs().await.unwrap(),
            reopened.model_prices().await.unwrap()
        ]),
        before
    );
}

#[tokio::test]
async fn registration_preserves_custom_prices_disabled_models_tombstones_and_request_snapshots() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("preservation-pg"))
        .await
        .unwrap();
    let models = [supported("gpt-6.1-sol")];
    let mut record = UsageRecord {
        id: "before-price-edit".into(),
        subject_id: "owner".into(),
        endpoint: "/v1/responses".into(),
        model: Some("gpt-6.1-sol".into()),
        status: "in_progress".into(),
        ..Default::default()
    };
    storage.insert_usage(&record).await.unwrap();
    let mut model = storage.model_config("gpt-6.1-sol").await.unwrap().unwrap();
    let mut custom = preset_model_prices("gpt-6.1-sol").unwrap().remove(0);
    custom.input_rate = 9_000_000;
    custom.source = "custom".into();
    model.enabled = false;
    assert!(
        storage
            .save_model_config(&model, &[custom], &[], Some(model.revision))
            .await
            .unwrap()
    );
    let before = json!([
        storage.model_config(&model.model).await.unwrap(),
        storage.model_prices().await.unwrap()
    ]);
    assert_eq!(storage.sync_supported_models(&models).await.unwrap(), 0);
    assert_eq!(
        json!([
            storage.model_config(&model.model).await.unwrap(),
            storage.model_prices().await.unwrap()
        ]),
        before
    );
    record.status = "completed".into();
    record.input_tokens = Some(1000);
    record.cached_tokens = Some(0);
    record.cache_write_tokens = Some(0);
    record.output_tokens = Some(0);
    storage.finish_usage(&record).await.unwrap();
    assert_eq!(
        storage
            .query_usage(&UsageFilter::default())
            .await
            .unwrap()
            .records[0]
            .cost_nano_usd,
        Some(2_000_000)
    );
    let current = storage.model_config(&model.model).await.unwrap().unwrap();
    assert!(
        storage
            .delete_model_config(&model.model, current.revision)
            .await
            .unwrap()
    );
    assert_eq!(storage.sync_supported_models(&models).await.unwrap(), 0);
    assert!(
        storage
            .model_config(&model.model)
            .await
            .unwrap()
            .unwrap()
            .deleted
    );
    assert!(
        !storage
            .model_prices()
            .await
            .unwrap()
            .iter()
            .any(|p| p.model == model.model)
    );
}

#[tokio::test]
async fn newly_verified_prices_enable_only_untouched_pending_records() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("pending-pg"))
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_catalog(model,platform,kind,enabled) VALUES('gpt-pending','chatgpt','text',FALSE)")
        .execute(storage.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE model_catalog SET revision=1 WHERE model='gpt-pending'")
        .execute(storage.pool())
        .await
        .unwrap();
    // A pending marker alone cannot price a model the table does not know.
    sqlx::query("INSERT INTO meta(key,value) VALUES('model_preset_pending:gpt-pending','1')")
        .execute(storage.pool())
        .await
        .unwrap();
    assert_eq!(
        storage
            .sync_supported_models(&[supported("gpt-pending")])
            .await
            .unwrap(),
        0
    );
    // An unmarked disabled record of a priced model is an administrator decision.
    let mut sol = storage.model_config("gpt-6-sol").await.unwrap().unwrap();
    sol.enabled = false;
    let prices = storage
        .model_prices()
        .await
        .unwrap()
        .into_iter()
        .filter(|p| p.model == "gpt-6-sol")
        .collect::<Vec<_>>();
    assert!(
        storage
            .save_model_config(&sol, &prices, &[], Some(sol.revision))
            .await
            .unwrap()
    );
    sqlx::query("DELETE FROM model_prices WHERE model='gpt-6-sol'")
        .execute(storage.pool())
        .await
        .unwrap();
    assert_eq!(
        storage
            .sync_supported_models(&[supported("gpt-6-sol")])
            .await
            .unwrap(),
        0
    );
    assert!(
        !storage
            .model_config("gpt-6-sol")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
    // A seeded, never edited, unpriced row carries the pending marker and is enabled.
    sqlx::query("INSERT INTO model_catalog(model,platform,kind,enabled) VALUES('gpt-5.6-sol-copy','chatgpt','text',FALSE)")
        .execute(storage.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO meta(key,value) VALUES('model_preset_pending:gpt-5.6-sol-copy','1')")
        .execute(storage.pool())
        .await
        .unwrap();
    let mut copy = supported("gpt-5.6-sol-copy");
    copy.preset_key = Some("gpt-5.6-sol".into());
    assert_eq!(
        storage
            .sync_supported_models(&[copy.clone()])
            .await
            .unwrap(),
        1
    );
    assert!(
        storage
            .model_config("gpt-5.6-sol-copy")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
    assert_eq!(storage.sync_supported_models(&[copy]).await.unwrap(), 0);
}

#[tokio::test]
async fn invalid_seed_rolls_back_the_entire_registration() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("atomic-pg"))
        .await
        .unwrap();
    assert!(
        storage
            .sync_supported_models(&[supported("gpt-new"), supported("bad model")])
            .await
            .is_err()
    );
    assert!(storage.model_config("gpt-new").await.unwrap().is_none());
}

#[tokio::test]
async fn gpt55_global_tier_multipliers_and_cache_writes_keep_request_snapshots() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("gpt55-pg"))
        .await
        .unwrap();
    for (id, tier, input, writes, expected, status) in [
        (
            "standard",
            "standard",
            1000,
            1000,
            Some(5_000_000),
            "priced",
        ),
        (
            "fast-boundary",
            "fast",
            272_000,
            0,
            Some(3_400_000_000),
            "priced",
        ),
        (
            "fast-long",
            "fast",
            272_001,
            0,
            Some(6_800_025_000),
            "priced",
        ),
        (
            "standard-long",
            "standard",
            272_001,
            0,
            Some(2_720_010_000),
            "priced",
        ),
        (
            "flex-long",
            "flex",
            272_001,
            0,
            Some(1_360_005_000),
            "priced",
        ),
    ] {
        let mut usage = UsageRecord {
            id: id.into(),
            subject_id: "owner".into(),
            model: Some("gpt-5.5".into()),
            endpoint: "/v1/responses".into(),
            status: "in_progress".into(),
            service_tier: Some(tier.into()),
            ..Default::default()
        };
        storage.insert_usage(&usage).await.unwrap();
        // Editing today's multiplier after request start must not change its snapshot.
        sqlx::query(
            "UPDATE model_prices SET input_rate=input_rate*2,cache_write_rate=cache_write_rate*2 WHERE model='gpt-5.5' AND tier='fast'",
        )
        .execute(storage.pool())
        .await
        .unwrap();
        usage.input_tokens = Some(input);
        usage.output_tokens = Some(0);
        usage.cached_tokens = Some(0);
        usage.cache_write_tokens = Some(writes);
        usage.status = "completed".into();
        storage.finish_usage(&usage).await.unwrap();
        let rows = storage
            .query_usage(&UsageFilter::default())
            .await
            .unwrap()
            .records;
        let saved = rows.iter().find(|row| row.id == id).unwrap();
        assert_eq!(
            (saved.cost_nano_usd, saved.billing_status.as_str()),
            (expected, status),
            "{id}"
        );
        sqlx::query(
            "UPDATE model_prices SET input_rate=input_rate/2,cache_write_rate=cache_write_rate/2 WHERE model='gpt-5.5' AND tier='fast'",
        )
        .execute(storage.pool())
        .await
        .unwrap();
    }
}

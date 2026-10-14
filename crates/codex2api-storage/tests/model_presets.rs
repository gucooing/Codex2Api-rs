use codex2api_core::SupportedModel;
use codex2api_storage::{Storage, UsageFilter, UsageRecord, preset_model_prices};
use serde_json::json;

fn supported(model: &str) -> SupportedModel {
    SupportedModel {
        provider_id: "chatgpt".into(),
        model: model.into(),
        kind: "text".into(),
    }
}

#[tokio::test]
async fn new_models_are_priced_once_unknown_prices_stay_disabled_and_restarts_are_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("models.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let models = [
        supported("gpt-6.1-sol"),
        supported("gpt-5.5"),
        supported("future-supported"),
    ];
    let (a, b) = tokio::join!(
        storage.sync_supported_models(&models),
        storage.sync_supported_models(&models)
    );
    assert_eq!(a.unwrap() + b.unwrap(), 3);
    let config = storage
        .model_config("chatgpt", "gpt-6.1-sol")
        .await
        .unwrap()
        .unwrap();
    assert!(config.enabled);
    assert!(
        storage
            .model_config("chatgpt", "gpt-5.5")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
    let pending = storage
        .model_config("chatgpt", "future-supported")
        .await
        .unwrap()
        .unwrap();
    assert!(!pending.enabled);
    let prices = storage.model_prices("chatgpt").await.unwrap();
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
    let before = json!([storage.model_configs("chatgpt").await.unwrap(), prices]);
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    assert_eq!(reopened.sync_supported_models(&models).await.unwrap(), 0);
    assert_eq!(
        json!([
            reopened.model_configs("chatgpt").await.unwrap(),
            reopened.model_prices("chatgpt").await.unwrap()
        ]),
        before
    );
}

#[tokio::test]
async fn registration_preserves_custom_prices_disabled_models_tombstones_and_request_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("preservation.sqlite"))
        .await
        .unwrap();
    let models = [supported("gpt-6.1-sol")];
    storage.sync_supported_models(&models).await.unwrap();
    let mut record = UsageRecord {
        id: "before-price-edit".into(),
        subject_id: "owner".into(),
        endpoint: "/v1/responses".into(),
        model: Some("gpt-6.1-sol".into()),
        status: "in_progress".into(),
        ..Default::default()
    };
    storage.insert_usage(&record).await.unwrap();
    let mut model = storage
        .model_config("chatgpt", "gpt-6.1-sol")
        .await
        .unwrap()
        .unwrap();
    let mut custom = preset_model_prices("chatgpt", "gpt-6.1-sol")
        .unwrap()
        .remove(0);
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
        storage.model_config("chatgpt", &model.model).await.unwrap(),
        storage.model_prices("chatgpt").await.unwrap()
    ]);
    assert_eq!(storage.sync_supported_models(&models).await.unwrap(), 0);
    assert_eq!(
        json!([
            storage.model_config("chatgpt", &model.model).await.unwrap(),
            storage.model_prices("chatgpt").await.unwrap()
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
    let current = storage
        .model_config("chatgpt", &model.model)
        .await
        .unwrap()
        .unwrap();
    assert!(
        storage
            .delete_model_config("chatgpt", &model.model, current.revision)
            .await
            .unwrap()
    );
    assert_eq!(storage.sync_supported_models(&models).await.unwrap(), 0);
    assert!(
        storage
            .model_config("chatgpt", &model.model)
            .await
            .unwrap()
            .unwrap()
            .deleted
    );
    assert!(
        !storage
            .model_prices("chatgpt")
            .await
            .unwrap()
            .iter()
            .any(|p| p.model == model.model)
    );
}

#[tokio::test]
async fn newly_verified_prices_enable_only_untouched_pending_records() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("pending.sqlite"))
        .await
        .unwrap();
    sqlx::query("INSERT INTO model_catalog(provider_id,model,kind,enabled) VALUES('chatgpt','gpt-6.1-sol','text',0)").execute(storage.pool()).await.unwrap();
    let models = [supported("gpt-6.1-sol")];
    assert_eq!(
        storage.sync_supported_models(&models).await.unwrap(),
        0,
        "an unmarked disabled record is not ours to enable"
    );
    sqlx::query(
        "INSERT INTO meta(key,value) VALUES('model_preset_pending:chatgpt:gpt-6.1-sol','1')",
    )
    .execute(storage.pool())
    .await
    .unwrap();
    assert_eq!(storage.sync_supported_models(&models).await.unwrap(), 1);
    assert!(
        storage
            .model_config("chatgpt", "gpt-6.1-sol")
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
    assert_eq!(storage.sync_supported_models(&models).await.unwrap(), 0);
}

#[tokio::test]
async fn invalid_seed_rolls_back_the_entire_registration() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("atomic.sqlite"))
        .await
        .unwrap();
    assert!(
        storage
            .sync_supported_models(&[supported("gpt-6.1-sol"), supported("bad model")])
            .await
            .is_err()
    );
    assert!(
        storage
            .model_config("chatgpt", "gpt-6.1-sol")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn gpt55_global_tier_multipliers_and_cache_writes_keep_request_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("gpt55.sqlite"))
        .await
        .unwrap();
    storage
        .sync_supported_models(&[supported("gpt-5.5")])
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

#[tokio::test]
async fn global_fast_migration_repairs_only_unchanged_defaults_and_preserves_history() {
    use sqlx::{
        migrate::Migrator,
        sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    };
    use std::borrow::Cow;
    static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");
    for custom in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("before-global-fast.sqlite");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true)
                    .foreign_keys(false),
            )
            .await
            .unwrap();
        Migrator {
            migrations: Cow::Owned(
                MIGRATIONS
                    .iter()
                    .filter(|m| m.version <= 80)
                    .cloned()
                    .collect(),
            ),
            ..Migrator::DEFAULT
        }
        .run(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO model_catalog(provider_id,model,kind,enabled,revision) VALUES('chatgpt','gpt-5.5','text',0,7)").execute(&pool).await.unwrap();
        let mut old_prices = preset_model_prices("chatgpt", "gpt-5.5").unwrap();
        old_prices.retain(|p| p.tier != "fast" || p.min_input_tokens == 0);
        for price in &mut old_prices {
            price.source = if custom {
                "custom"
            } else {
                "preset:2026-10-09"
            }
            .into();
            if price.tier == "fast" {
                price.max_input_tokens = Some(272000);
            }
            sqlx::query("INSERT INTO model_prices(provider_id,model,tier,min_input_tokens,max_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source,revision) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                .bind(&price.provider_id).bind(&price.model).bind(&price.tier).bind(price.min_input_tokens).bind(price.max_input_tokens).bind(price.input_rate).bind(price.cached_rate).bind(price.cache_write_rate).bind(price.output_rate).bind(&price.source).bind(4_i64).execute(&pool).await.unwrap();
        }
        let snapshot = serde_json::to_string(&old_prices).unwrap();
        sqlx::query("INSERT INTO usage_records(id,account_id,account_name,subject_id,subject_name,endpoint,transport,model,requested_at_ms,status,pricing_snapshot_json,cost_nano_usd,billing_status) VALUES('history','','','owner','','/v1/responses','http','gpt-5.5',0,'completed',?,3400000000,'priced')")
            .bind(&snapshot).execute(&pool).await.unwrap();
        pool.close().await;
        let storage = Storage::open(&path).await.unwrap();
        let rows = storage.model_prices("chatgpt").await.unwrap();
        let fast: Vec<_> = rows
            .iter()
            .filter(|p| p.model == "gpt-5.5" && p.tier == "fast")
            .collect();
        assert_eq!(fast.len(), if custom { 1 } else { 2 });
        assert_eq!(
            fast[0].max_input_tokens,
            if custom { Some(272000) } else { None }
        );
        if !custom {
            assert_eq!(fast[1].input_rate, 25_000_000);
            assert_eq!(fast[1].output_rate, 112_500_000);
        }
        let config = storage
            .model_config("chatgpt", "gpt-5.5")
            .await
            .unwrap()
            .unwrap();
        assert!(!config.enabled);
        assert_eq!(config.revision, if custom { 7 } else { 8 });
        let history: (String, i64) = sqlx::query_as(
            "SELECT pricing_snapshot_json,cost_nano_usd FROM usage_records WHERE id='history'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap();
        assert_eq!(history, (snapshot, 3_400_000_000));
        storage.close().await;
        let reopened = Storage::open(&path).await.unwrap();
        assert_eq!(
            reopened
                .model_config("chatgpt", "gpt-5.5")
                .await
                .unwrap()
                .unwrap()
                .revision,
            config.revision
        );
    }
}

use codex2api_storage::{ImagePrice, ImageUsage, ModelConfig, Storage, UsageFilter, UsageRecord};

#[test]
fn screen_resolution_bands_accept_dynamic_dimensions_and_both_orientations() {
    for (size, tier) in [
        ("1024x1024", "1K"),
        ("1920x1080", "2K"),
        ("2560x1440", "2K"),
        ("1440x2560", "2K"),
        ("2304x1296", "2K"),
        ("3840x2160", "4K"),
        ("2160x3840", "4K"),
        ("4096x1716", "4K"),
        ("7680x4320", "8K"),
        ("2k", "2K"),
    ] {
        assert_eq!(
            codex2api_storage::image_resolution_tier(size).as_deref(),
            Some(tier),
            "{size}"
        );
    }
    for size in ["auto", "0x1024", "9000x1000", "invalid"] {
        assert!(codex2api_storage::image_resolution_tier(size).is_none());
    }
}

#[tokio::test]
async fn old_fixed_resolution_snapshots_keep_their_original_price() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("legacy-size.sqlite"))
        .await
        .unwrap();
    let model = ModelConfig {
        provider_id: "chatgpt".into(),
        model: "image-test".into(),
        kind: "image".into(),
        enabled: true,
        deleted: false,
        revision: 0,
    };
    let mut price = ImagePrice {
        provider_id: "chatgpt".into(),
        model: model.model.clone(),
        resolution: "1K".into(),
        price_nano_usd: 40_000_000,
    };
    storage
        .save_model_config(&model, &[], &[price.clone()], None)
        .await
        .unwrap();
    sqlx::query("UPDATE model_image_prices SET resolution='1024x1024' WHERE model='image-test'")
        .execute(storage.pool())
        .await
        .unwrap();
    let mut record = UsageRecord {
        id: "legacy".into(),
        subject_id: "owner".into(),
        endpoint: "/v1/images/generations".into(),
        model: Some(model.model.clone()),
        status: "in_progress".into(),
        ..Default::default()
    };
    storage.insert_usage(&record).await.unwrap();
    price.price_nano_usd = 400_000_000;
    storage
        .save_model_config(&model, &[], &[price], Some(1))
        .await
        .unwrap();
    record.status = "completed".into();
    record.image_count = Some(1);
    record.image_usage_json = Some(r#"[{"resolution":"1024x1024","count":1}]"#.into());
    storage.finish_usage(&record).await.unwrap();
    assert_eq!(
        storage
            .query_usage(&UsageFilter::default())
            .await
            .unwrap()
            .records[0]
            .cost_nano_usd,
        Some(40_000_000)
    );
}

#[tokio::test]
async fn image_billing_uses_resolution_counts_and_start_prices_after_edit_or_delete() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("images.sqlite"))
        .await
        .unwrap();
    let model = ModelConfig {
        provider_id: "chatgpt".into(),
        model: "image-test".into(),
        kind: "image".into(),
        enabled: true,
        deleted: false,
        revision: 0,
    };
    let prices = vec![
        ImagePrice {
            provider_id: "chatgpt".into(),
            model: model.model.clone(),
            resolution: "1K".into(),
            price_nano_usd: 40_000_000,
        },
        ImagePrice {
            provider_id: "chatgpt".into(),
            model: model.model.clone(),
            resolution: "2K".into(),
            price_nano_usd: 80_000_000,
        },
    ];
    assert!(
        storage
            .save_model_config(&model, &[], &prices, None)
            .await
            .unwrap()
    );
    storage
        .save_virtual_account(&codex2api_storage::VirtualAccount {
            provider_id: "chatgpt".into(),
            id: "virtual-one".into(),
            username: "virtual-one".into(),
            password_hash: "unused".into(),
            name: "Virtual".into(),
            email: "virtual@example.test".into(),
            plan_type: "plus".into(),
            plan_id: "plus".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .await
        .unwrap();
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["primary_cost_limit_usd"] = serde_json::json!(0.1);
    plan.config["weekly_cost_limit_usd"] = serde_json::json!(0.5);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let mut record = UsageRecord {
        id: "generated".into(),
        subject_id: "virtual-one".into(),
        endpoint: "/v1/images/generations".into(),
        model: Some(model.model.clone()),
        status: "in_progress".into(),
        requested_at_ms: chrono::Utc::now().timestamp_millis(),
        ..Default::default()
    };
    storage.insert_usage(&record).await.unwrap();
    let mut changed = prices.clone();
    changed[0].price_nano_usd = 400_000_000;
    assert!(
        storage
            .save_model_config(&model, &[], &changed, Some(1))
            .await
            .unwrap()
    );
    assert!(
        !storage
            .save_model_config(&model, &[], &changed, Some(1))
            .await
            .unwrap()
    );
    assert!(
        storage
            .delete_model_config("chatgpt", &model.model, 2)
            .await
            .unwrap()
    );
    record.image_count = Some(3);
    record.image_usage_json = Some(
        serde_json::to_string(&vec![
            ImageUsage {
                resolution: Some("1024x1024".into()),
                count: 2,
            },
            ImageUsage {
                resolution: Some("1536x1024".into()),
                count: 1,
            },
        ])
        .unwrap(),
    );
    record.status = "completed".into();
    storage.finish_usage(&record).await.unwrap();
    record.image_count = Some(9);
    storage.finish_usage(&record).await.unwrap();
    let saved = storage
        .query_usage(&UsageFilter::default())
        .await
        .unwrap()
        .records
        .remove(0);
    assert_eq!(saved.image_count, Some(3));
    assert_eq!(saved.cost_nano_usd, Some(160_000_000));
    assert_eq!(saved.billing_status, "priced");
    assert_eq!(
        storage.billing_summary("virtual-one").await.unwrap()["used_usd"],
        "0.16"
    );
    let quota = storage.virtual_quota("virtual-one").await.unwrap();
    assert_eq!(quota["rate_limit"]["allowed"], false);
    assert_eq!(quota["rate_limit"]["primary_window"]["used_percent"], 100);
    assert_eq!(quota["rate_limit"]["secondary_window"]["used_percent"], 32);
    assert!(
        storage
            .model_configs("chatgpt")
            .await
            .unwrap()
            .iter()
            .all(|m| m.model != "image-test")
    );
    assert!(
        storage
            .unavailable_models("chatgpt")
            .await
            .unwrap()
            .contains(&model.model)
    );
    assert!(
        !storage
            .virtual_plan_model_choices("chatgpt")
            .await
            .unwrap()
            .contains(&model.model)
    );
    assert!(
        storage
            .save_model_config(&model, &[], &prices, None)
            .await
            .unwrap()
    );
    let current = storage
        .model_config("chatgpt", &model.model)
        .await
        .unwrap()
        .unwrap();
    assert!(current.revision > 2);
    assert!(current.enabled);
}

#[tokio::test]
async fn image_unknowns_are_explicit_and_failures_without_images_are_not_charged() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("image-errors.sqlite"))
        .await
        .unwrap();
    let model = ModelConfig {
        provider_id: "chatgpt".into(),
        model: "image-test".into(),
        kind: "image".into(),
        enabled: true,
        deleted: false,
        revision: 0,
    };
    let prices = vec![ImagePrice {
        provider_id: "chatgpt".into(),
        model: model.model.clone(),
        resolution: "1K".into(),
        price_nano_usd: 1,
    }];
    storage
        .save_model_config(&model, &[], &prices, None)
        .await
        .unwrap();
    for (id, status, usage, count, expected, cost) in [
        (
            "unknown-size",
            "completed",
            Some(r#"[{"resolution":null,"count":1}]"#),
            Some(1),
            "missing_resolution",
            None,
        ),
        (
            "unknown-price",
            "completed",
            Some(r#"[{"resolution":"2048x2048","count":1}]"#),
            Some(1),
            "unpriced",
            None,
        ),
        (
            "failure",
            "failed",
            Some("[]"),
            Some(0),
            "not_charged",
            Some(0),
        ),
        (
            "transport-failure",
            "failed",
            None,
            None,
            "missing_usage",
            None,
        ),
        ("missing", "completed", None, None, "missing_usage", None),
        ("empty", "completed", Some("[]"), Some(0), "priced", Some(0)),
        (
            "precision",
            "completed",
            Some(r#"[{"resolution":"1024x1024","count":2}]"#),
            Some(2),
            "priced",
            Some(2),
        ),
    ] {
        let mut r = UsageRecord {
            id: id.into(),
            subject_id: "owner".into(),
            model: Some(model.model.clone()),
            endpoint: "/v1/images/edits".into(),
            status: "in_progress".into(),
            ..Default::default()
        };
        storage.insert_usage(&r).await.unwrap();
        r.status = status.into();
        r.image_count = count;
        r.image_usage_json = usage.map(str::to_owned);
        storage.finish_usage(&r).await.unwrap();
        let rows = storage
            .query_usage(&UsageFilter::default())
            .await
            .unwrap()
            .records;
        let saved = rows.iter().find(|r| r.id == id).unwrap();
        assert_eq!(saved.billing_status, expected, "{id}");
        assert_eq!(saved.cost_nano_usd, cost, "{id}");
    }
    let invalid = [ImagePrice {
        provider_id: "chatgpt".into(),
        model: model.model.clone(),
        resolution: "auto".into(),
        price_nano_usd: 1,
    }];
    assert!(
        storage
            .save_model_config(&model, &[], &invalid, Some(1))
            .await
            .is_err()
    );
    let duplicate = [prices[0].clone(), prices[0].clone()];
    assert!(
        storage
            .save_model_config(&model, &[], &duplicate, Some(1))
            .await
            .is_err()
    );
}

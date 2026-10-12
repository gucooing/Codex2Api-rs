use chrono::Utc;
use codex2api_storage::{
    ApiGroup, ApiListQuery, ApiModelRule, ApiPrincipal, ImagePrice, ModelConfig, ModelPrice,
    NewSupplierAccount, Storage, StorageError, UsageRecord, User, UserKind, WalletAdjustment,
};
use serde_json::json;

async fn setup() -> (tempfile::TempDir, Storage, User, ApiGroup, String) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("api.sqlite")).await.unwrap();
    let user = User {
        id: "regular".into(),
        username: "regular".into(),
        password_hash: "fixture-hash".into(),
        name: "Regular".into(),
        email: "regular@example.test".into(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: Utc::now().to_rfc3339(),
        kind: UserKind::Regular,
    };
    storage.save_user(&user, None).await.unwrap();
    let admin = storage.require_admin_user().await.unwrap();
    storage
        .adjust_user_wallet(
            &user.id,
            admin.id,
            WalletAdjustment {
                request_id: "fund".into(),
                amount_cents: 100,
                expected_revision: 1,
                reason: None,
            },
        )
        .await
        .unwrap();
    let supplier = storage
        .create_account(NewSupplierAccount::pending_identity(
            "installation",
            "fixture",
            "fixture",
            "fixture",
            "fixture",
            "fixture",
            "",
            "{}",
        ))
        .await
        .unwrap();
    storage
        .save_supplier_tag("pool", "chatgpt", "Private supplier pool")
        .await
        .unwrap();
    storage
        .edit_supplier_tags(&[supplier.id.clone()], &["pool".into()], false)
        .await
        .unwrap();
    for name in ["api-text", "api-other", "api-unpriced"] {
        let config = ModelConfig {
            provider_id: "chatgpt".into(),
            model: name.into(),
            kind: "text".into(),
            enabled: true,
            deleted: false,
            revision: 0,
        };
        let prices = vec![ModelPrice {
            provider_id: "chatgpt".into(),
            model: name.into(),
            tier: "standard".into(),
            min_input_tokens: 0,
            max_input_tokens: None,
            input_rate: 1_000_000,
            cached_rate: 100_000,
            cache_write_rate: 1_250_000,
            output_rate: 2_000_000,
            source: "custom".into(),
            revision: 0,
        }];
        storage
            .save_model_config(&config, &prices, &[], None)
            .await
            .unwrap();
        if name == "api-unpriced" {
            sqlx::query(
                "DELETE FROM model_prices WHERE provider_id='chatgpt' AND model='api-unpriced'",
            )
            .execute(storage.pool())
            .await
            .unwrap();
        }
        if name != "api-other" {
            storage
                .add_supplier_model(&supplier.id, name)
                .await
                .unwrap();
        }
    }
    let group = ApiGroup {
        id: "group".into(),
        name: "API group".into(),
        description: "Public description".into(),
        provider_id: "chatgpt".into(),
        enabled: true,
        tag_ids: vec!["pool".into()],
        allowed_models: vec![],
        multiplier: "1.5".into(),
        concurrency_limit: 5,
        rpm_limit: 60,
        model_rules: vec![],
        revision: 0,
    };
    storage.save_api_group(&group, None).await.unwrap();
    let group = storage.api_group(&group.id).await.unwrap().unwrap();
    (dir, storage, user, group, supplier.id)
}
async fn credential(storage: &Storage, user: &User, group: &ApiGroup) -> (String, ApiPrincipal) {
    let created = storage
        .user_store()
        .create_api_key(&user.id, "Primary key", &group.id)
        .await
        .unwrap();
    let principal = storage
        .authenticate_api_key(&created.secret)
        .await
        .unwrap()
        .unwrap();
    (created.secret, principal)
}
fn request(id: &str) -> UsageRecord {
    UsageRecord {
        id: id.into(),
        model: Some("api-text".into()),
        endpoint: "/v1/chat/completions".into(),
        provider_id: "chatgpt".into(),
        requested_at_ms: Utc::now().timestamp_millis(),
        status: "started".into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn api_keys_catalogs_and_queries_enforce_regular_ownership_and_secret_isolation() {
    let (_dir, storage, user, group, supplier) = setup().await;
    let (secret, principal) = credential(&storage, &user, &group).await;
    assert_eq!(secret.len(), 71);
    let (hash, prefix): (String, String) =
        sqlx::query_as("SELECT token_hash,key_prefix FROM api_keys WHERE id=?")
            .bind(&principal.key_id)
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(hash, codex2api_storage::hash_token(&secret));
    assert!(!hash.contains(&secret));
    assert_eq!(prefix.len(), 15);
    assert!(
        storage
            .authenticate_api_key("Bearer invalid")
            .await
            .unwrap()
            .is_none()
    );
    let key_json = serde_json::to_string(
        &storage
            .user_store()
            .api_keys(&user.id, &ApiListQuery::default())
            .await
            .unwrap(),
    )
    .unwrap();
    for private in [
        &secret,
        &hash,
        &supplier,
        "token_hash",
        "username",
        "user_id",
    ] {
        assert!(!key_json.contains(private));
    }
    let catalog = storage
        .user_store()
        .api_catalog(
            &user.id,
            &ApiListQuery {
                group_id: group.id.clone(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(catalog["total"], 2);
    assert_eq!(catalog["items"][0]["model"], "api-text");
    assert_eq!(
        catalog["items"][0]["prices"][0]["input_usd_per_million"],
        "1.5"
    );
    assert_eq!(
        catalog["items"][0]["prices"][0]["base_input_usd_per_million"],
        "1"
    );
    assert!(catalog.to_string().find("tag_ids").is_none());
    assert!(!catalog.to_string().contains(&supplier));
    let mut second = user.clone();
    second.id = "another".into();
    second.username = "another".into();
    storage.save_user(&second, None).await.unwrap();
    assert_eq!(
        storage
            .user_store()
            .api_keys(
                &second.id,
                &ApiListQuery {
                    user_id: user.id.clone(),
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .total,
        0
    );
    assert!(
        !storage
            .user_store()
            .revoke_api_key(&second.id, &principal.key_id, 1)
            .await
            .unwrap()
    );
    assert!(
        !storage
            .user_store()
            .update_api_key(&second.id, &principal.key_id, "Stolen", false, 1)
            .await
            .unwrap()
    );
    sqlx::query("INSERT INTO accounts(id,account_type,username,password_hash,enabled,created_at,updated_at) VALUES('virtual','user','virtual','hash',1,'now','now')").execute(storage.pool()).await.unwrap();
    sqlx::query("INSERT INTO users(id,name,email,kind) VALUES('virtual','Virtual','','virtual')")
        .execute(storage.pool())
        .await
        .unwrap();
    assert!(
        storage
            .user_store()
            .create_api_key("virtual", "key", &group.id)
            .await
            .is_err()
    );
    assert!(
        storage
            .user_store()
            .api_catalog("virtual", &Default::default())
            .await
            .is_err()
    );
    assert!(sqlx::query("INSERT INTO api_keys(id,user_id,group_id,name,token_hash,key_prefix,created_at) VALUES('bad','virtual','group','bad','bad','bad','now')").execute(storage.pool()).await.is_err());
    assert!(
        sqlx::query("UPDATE api_keys SET user_id='another' WHERE id=?")
            .bind(&principal.key_id)
            .execute(storage.pool())
            .await
            .is_err()
    );
    assert!(
        storage
            .user_store()
            .revoke_api_key(&user.id, &principal.key_id, 1)
            .await
            .unwrap()
    );
    assert!(
        storage
            .authenticate_api_key(&secret)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        sqlx::query("UPDATE api_keys SET revoked_at=NULL,enabled=1 WHERE id=?")
            .bind(&principal.key_id)
            .execute(storage.pool())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn api_settlement_snapshots_prices_rounds_once_accumulates_subcents_and_replays() {
    let (_dir, storage, user, group, supplier) = setup().await;
    let (_, principal) = credential(&storage, &user, &group).await;
    let mut first = request("first");
    storage.start_api_request(&principal, &first).await.unwrap();
    // Changing both policy and catalog cannot reprice a request already admitted.
    let mut changed = group.clone();
    changed.multiplier = "10".into();
    storage
        .save_api_group(&changed, Some(group.revision))
        .await
        .unwrap();
    let mut price = storage
        .model_prices("chatgpt")
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.model == "api-text")
        .unwrap();
    price.input_rate = 10_000_000;
    storage.save_model_price(&price).await.unwrap();
    first.status = "completed".into();
    first.account_id = supplier.clone();
    first.input_tokens = Some(6000);
    first.cached_tokens = Some(2000);
    first.cache_write_tokens = Some(1000);
    first.output_tokens = Some(1000);
    first.reasoning_tokens = Some(500);
    let settlement = storage.finish_api_request(&first).await.unwrap();
    assert_eq!(settlement.cost_nano_usd, Some(9_675_000));
    assert_eq!(settlement.wallet_debit_cents, 0);
    let supplier_usage = storage
        .supplier_cycle_usage(&supplier, 0, Utc::now().timestamp_millis() + 1)
        .await
        .unwrap();
    assert_eq!(supplier_usage.request_count, 1);
    assert_eq!(supplier_usage.cost_nano_usd, Some(6_450_000));
    assert_eq!(
        storage
            .finish_api_request(&first)
            .await
            .unwrap()
            .wallet_debit_cents,
        0
    );
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        100
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT pending_nano_usd FROM api_wallet_state WHERE user_id='regular'"
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        9_675_000
    );
    let mut stale = request("stale");
    assert!(matches!(
        storage.start_api_request(&principal, &stale).await,
        Err(StorageError::Api {
            code: "api_group_changed",
            ..
        })
    ));
    let new = storage.api_group(&group.id).await.unwrap().unwrap();
    let principal = ApiPrincipal {
        group: new,
        ..principal
    };
    stale.id = "second".into();
    storage.start_api_request(&principal, &stale).await.unwrap();
    stale.input_tokens = Some(100);
    stale.output_tokens = Some(0);
    stale.status = "completed".into();
    let result = storage.finish_api_request(&stale).await.unwrap();
    assert_eq!(result.cost_nano_usd, Some(10_000_000));
    assert_eq!(result.wallet_debit_cents, 1);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        99
    );
    assert_eq!(
        storage
            .finish_api_request(&stale)
            .await
            .unwrap()
            .wallet_debit_cents,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM wallet_entries WHERE kind='api_usage'")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        1
    );
    assert!(
        sqlx::query("UPDATE api_usage_records SET cost_nano_usd=0 WHERE id='second'")
            .execute(storage.pool())
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM api_usage_records WHERE id='second'")
            .execute(storage.pool())
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM usage_records")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    let usage = storage
        .user_store()
        .api_usage(&user.id, &Default::default())
        .await
        .unwrap();
    assert_eq!(usage["total"], 2);
    assert_eq!(usage["wallet_pending_nano_usd"], 9_675_000);
    assert!(!usage.to_string().contains("supplier_account_id"));
    let mut missing = request("missing-price");
    missing.model = Some("api-unpriced".into());
    assert!(matches!(
        storage.start_api_request(&principal, &missing).await,
        Err(StorageError::Api {
            code: "model_price_unavailable",
            ..
        })
    ));
    let admin = storage.require_admin_user().await.unwrap();
    let current = storage.user(&user.id).await.unwrap().unwrap();
    assert!(
        storage
            .adjust_user_wallet(
                &user.id,
                admin.id,
                WalletAdjustment {
                    request_id: "cannot-spend-remainder".into(),
                    amount_cents: -99,
                    expected_revision: current.revision,
                    reason: None
                }
            )
            .await
            .is_err()
    );
    storage
        .adjust_user_wallet(
            &user.id,
            admin.id,
            WalletAdjustment {
                request_id: "reserve-remainder".into(),
                amount_cents: -98,
                expected_revision: current.revision,
                reason: None,
            },
        )
        .await
        .unwrap();
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.allow_purchase = true;
    plan.config["sale_price_usd"] = json!("0.01");
    plan.config["duration_days"] = json!(30);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    let plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    let subscription = storage
        .user_subscriptions(Some(&user.id), true)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.provider_id == "chatgpt")
        .unwrap();
    let preview = storage
        .checkout_preview(
            &user.id,
            codex2api_storage::CheckoutInput {
                plan_id: plan.id,
                plan_revision: plan.revision,
                subscription_revision: Some(subscription.revision),
                payment_method: "wallet".into(),
                coupon_code: String::new(),
            },
        )
        .await
        .unwrap();
    let order = storage
        .create_subscription_order(codex2api_storage::OrderRequest {
            user_id: &user.id,
            preview_token: preview["preview_token"].as_str().unwrap(),
        })
        .await
        .unwrap();
    assert!(matches!(
        storage.pay_subscription_order(&user.id, &order.id).await,
        Err(StorageError::Checkout {
            code: "insufficient_balance",
            ..
        })
    ));
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        1
    );
}

#[tokio::test]
async fn concurrent_settlement_keeps_overdraft_debt_and_blocks_new_paid_admissions() {
    let (_dir, storage, user, mut group, _) = setup().await;
    group.model_rules = vec![ApiModelRule {
        model: "api-text".into(),
        multiplier: Some("2".into()),
        concurrency_limit: 2,
        rpm_limit: 10,
    }];
    storage
        .save_api_group(&group, Some(group.revision))
        .await
        .unwrap();
    let group = storage.api_group(&group.id).await.unwrap().unwrap();
    let (_, principal) = credential(&storage, &user, &group).await;
    let catalog = storage.api_group_models(&group.id).await.unwrap();
    assert_eq!(catalog[0].multiplier, "2");
    assert_eq!(catalog[0].concurrency_limit, 2);
    assert_eq!(catalog[0].group_concurrency_limit, 5);
    let mut a = request("over-a");
    let mut b = request("over-b");
    storage.start_api_request(&principal, &a).await.unwrap();
    storage.start_api_request(&principal, &b).await.unwrap();
    for r in [&mut a, &mut b] {
        r.input_tokens = Some(400_000);
        r.output_tokens = Some(0);
        r.status = "completed".into();
    }
    let (a, b) = tokio::join!(
        storage.finish_api_request(&a),
        storage.finish_api_request(&b)
    );
    assert_eq!(
        a.unwrap().wallet_debit_cents + b.unwrap().wallet_debit_cents,
        100
    );
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT pending_nano_usd FROM api_wallet_state WHERE user_id='regular'"
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        600_000_000
    );
    assert!(matches!(
        storage
            .start_api_request(&principal, &request("after"))
            .await,
        Err(StorageError::Api {
            code: "insufficient_balance",
            ..
        })
    ));
    let unmatched = storage
        .user_store()
        .api_usage(
            &user.id,
            &ApiListQuery {
                key_id: "other-key".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(unmatched["total"], 0);
    assert_eq!(unmatched["summary"]["cost_nano_usd"], 0);
    let admin = storage.require_admin_user().await.unwrap();
    for (id, amount, pending, balance) in
        [("repay-part", 30, 300_000_000, 0), ("repay-rest", 31, 0, 1)]
    {
        let current = storage.user(&user.id).await.unwrap().unwrap();
        storage
            .adjust_user_wallet(
                &user.id,
                admin.id,
                WalletAdjustment {
                    request_id: id.into(),
                    amount_cents: amount,
                    expected_revision: current.revision,
                    reason: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
            balance
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT pending_nano_usd FROM api_wallet_state WHERE user_id='regular'"
            )
            .fetch_one(storage.pool())
            .await
            .unwrap(),
            pending
        );
    }
    let debt_entries:i64=sqlx::query_scalar("SELECT COUNT(*) FROM wallet_entries w JOIN api_usage_records r ON r.id=w.api_usage_record_id AND r.user_id=w.user_id WHERE w.request_id LIKE 'api-debt:%' AND r.cost_nano_usd>0").fetch_one(storage.pool()).await.unwrap();
    assert_eq!(debt_entries, 2);
    storage
        .start_api_request(&principal, &request("interrupted"))
        .await
        .unwrap();
    assert_eq!(storage.recover_interrupted_api_usage().await.unwrap(), 1);
    assert_eq!(storage.recover_interrupted_api_usage().await.unwrap(), 0);
    let interruption: (String, Option<i64>) = sqlx::query_as(
        "SELECT billing_status,cost_nano_usd FROM api_usage_records WHERE id='interrupted'",
    )
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(interruption, ("interrupted".into(), None));
}

#[tokio::test]
async fn api_model_membership_pagination_and_provider_boundaries_follow_current_configuration() {
    let (_dir, storage, user, mut group, _) = setup().await;
    group.allowed_models = vec!["api-text".into()];
    storage
        .save_api_group(&group, Some(group.revision))
        .await
        .unwrap();
    let market = storage
        .user_store()
        .api_catalog(
            &user.id,
            &ApiListQuery {
                group_id: group.id.clone(),
                page: 99,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(market["total"], 1);
    assert_eq!(market["page"], 1);
    assert_eq!(market["items"].as_array().unwrap().len(), 1);
    let empty = storage
        .user_store()
        .api_catalog(
            &user.id,
            &ApiListQuery {
                group_id: group.id.clone(),
                search: "unavailable".into(),
                page: 99,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(empty["total"], 0);
    assert_eq!(empty["page"], 1);
    assert_eq!(
        storage
            .admin_api_keys(&ApiListQuery {
                page: 99,
                ..Default::default()
            })
            .await
            .unwrap()
            .page,
        1
    );
    assert_eq!(
        storage
            .api_group_page(&ApiListQuery {
                page: 99,
                ..Default::default()
            })
            .await
            .unwrap()
            .page,
        1
    );
    assert_eq!(
        storage
            .admin_api_usage(&ApiListQuery {
                page: 99,
                ..Default::default()
            })
            .await
            .unwrap()["page"],
        1
    );
    storage
        .save_supplier_tag("grok-pool", "grok", "Grok pool")
        .await
        .unwrap();
    let mut group = storage.api_group(&group.id).await.unwrap().unwrap();
    group.tag_ids = vec!["grok-pool".into()];
    assert!(
        storage
            .save_api_group(&group, Some(group.revision))
            .await
            .is_err()
    );
    assert!(
        sqlx::query("INSERT INTO api_group_tags(group_id,tag_id) VALUES('group','grok-pool')")
            .execute(storage.pool())
            .await
            .is_err()
    );
    assert!(storage.delete_supplier_tag("pool").await.is_err());
    assert!(
        storage
            .delete_api_group(&group.id, group.revision)
            .await
            .unwrap()
    );
    storage.delete_supplier_tag("pool").await.unwrap();
}

#[tokio::test]
async fn metered_api_migration_preserves_existing_wallet_entries_and_guards() {
    use sqlx::{
        migrate::Migrator,
        sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    };
    use std::borrow::Cow;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("before-api.sqlite");
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
    let migrations = sqlx::migrate!("./migrations");
    Migrator {
        migrations: Cow::Owned(
            migrations
                .iter()
                .filter(|m| m.version < 76)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
    .run(&pool)
    .await
    .unwrap();
    sqlx::raw_sql("INSERT INTO accounts(id,account_type,username,password_hash,enabled,created_at,updated_at) VALUES('kept','user','kept','hash',1,'2026-01-01','2026-01-01');
        INSERT INTO users(id,name,email,wallet_cents) VALUES('kept','Kept','kept@example.test',123);
        INSERT INTO wallet_entries(id,user_id,request_id,signature,kind,plan_id,plan_name,provider_id,amount_cents,balance_cents,created_at) VALUES('old-entry','kept','old-request','signature','order_payment','plus','Plus','chatgpt',-5,123,'2026-01-01');")
        .execute(&pool).await.unwrap();
    pool.close().await;
    let storage = Storage::open(&path).await.unwrap();
    let entries = storage.wallet_entries("kept").await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "old-entry");
    assert_eq!(entries[0].amount_cents, -5);
    assert_eq!(entries[0].balance_before_cents, 128);
    assert_eq!(entries[0].api_usage_record_id, None);
    assert_eq!(
        storage.user("kept").await.unwrap().unwrap().wallet_cents,
        123
    );
    assert!(
        sqlx::query("UPDATE wallet_entries SET amount_cents=-1 WHERE id='old-entry'")
            .execute(storage.pool())
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM wallet_entries WHERE id='old-entry'")
            .execute(storage.pool())
            .await
            .is_err()
    );
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(storage.pool())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(storage.recover_interrupted_api_usage().await.unwrap(), 0);
}

#[tokio::test]
async fn image_prices_and_unknown_measurements_remain_distinct_from_free_usage() {
    let (_dir, storage, user, group, supplier) = setup().await;
    storage
        .save_model_config(
            &ModelConfig {
                provider_id: "chatgpt".into(),
                model: "api-image".into(),
                kind: "image".into(),
                enabled: true,
                deleted: false,
                revision: 0,
            },
            &[],
            &[ImagePrice {
                provider_id: "chatgpt".into(),
                model: "api-image".into(),
                resolution: "1K".into(),
                price_nano_usd: 20_000_000,
            }],
            None,
        )
        .await
        .unwrap();
    storage
        .add_supplier_model(&supplier, "api-image")
        .await
        .unwrap();
    let (_, principal) = credential(&storage, &user, &group).await;
    let mut image = request("image");
    image.endpoint = "/v1/images/edits".into();
    image.model = Some("api-image".into());
    image.image_size = Some("unpriced-dimensions".into());
    assert!(matches!(
        storage.start_api_request(&principal, &image).await,
        Err(StorageError::Api {
            code: "model_price_unavailable",
            ..
        })
    ));
    image.image_size = Some("1024x1024".into());
    storage.start_api_request(&principal, &image).await.unwrap();
    image.status = "completed".into();
    image.image_count = Some(2);
    image.image_usage_json = Some(json!([{"resolution":"1024x1024","count":2}]).to_string());
    let settled = storage.finish_api_request(&image).await.unwrap();
    assert_eq!(settled.cost_nano_usd, Some(60_000_000));
    assert_eq!(settled.wallet_debit_cents, 6);
    let mut unknown = request("unknown");
    storage
        .start_api_request(&principal, &unknown)
        .await
        .unwrap();
    unknown.status = "completed".into();
    assert_eq!(
        storage
            .finish_api_request(&unknown)
            .await
            .unwrap()
            .cost_nano_usd,
        None
    );
    let mut failed = request("failed");
    storage
        .start_api_request(&principal, &failed)
        .await
        .unwrap();
    failed.status = "failed".into();
    let failed = storage.finish_api_request(&failed).await.unwrap();
    assert_eq!(failed.cost_nano_usd, Some(0));
    assert_eq!(failed.billing_status, "not_charged");
    let usage = storage
        .user_store()
        .api_usage(&user.id, &Default::default())
        .await
        .unwrap();
    assert!(usage["summary"]["cost_nano_usd"].is_null());
    assert_eq!(usage["summary"]["unknown_cost_requests"], 1);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        94
    );
    for (id, status, first_byte) in [
        ("truncated", Some(200), Some(10)),
        ("sent-no-response", None, None),
        ("server-error", Some(500), None),
    ] {
        let mut truncated = request(id);
        storage
            .start_api_request(&principal, &truncated)
            .await
            .unwrap();
        truncated.account_id = supplier.clone();
        truncated.http_status = status;
        truncated.first_byte_ms = first_byte;
        truncated.status = "failed".into();
        let settled = storage.finish_api_request(&truncated).await.unwrap();
        assert_eq!(settled.cost_nano_usd, None, "{id}");
        assert_eq!(settled.billing_status, "missing_usage", "{id}");
    }
    let mut rejected = request("rejected");
    storage
        .start_api_request(&principal, &rejected)
        .await
        .unwrap();
    rejected.account_id = supplier;
    rejected.http_status = Some(429);
    rejected.status = "failed".into();
    assert_eq!(
        storage
            .finish_api_request(&rejected)
            .await
            .unwrap()
            .billing_status,
        "not_charged"
    );
    let mut price = storage
        .model_prices("chatgpt")
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.model == "api-text")
        .unwrap();
    price.input_rate = 1;
    storage.save_model_price(&price).await.unwrap();
    let mut group = storage.api_group(&group.id).await.unwrap().unwrap();
    group.multiplier = "1000".into();
    storage
        .save_api_group(&group, Some(group.revision))
        .await
        .unwrap();
    let principal = ApiPrincipal {
        group: storage.api_group(&group.id).await.unwrap().unwrap(),
        ..principal
    };
    let mut tiny = request("sub-nano-base");
    storage.start_api_request(&principal, &tiny).await.unwrap();
    tiny.input_tokens = Some(1);
    tiny.output_tokens = Some(0);
    tiny.status = "completed".into();
    assert_eq!(
        storage
            .finish_api_request(&tiny)
            .await
            .unwrap()
            .cost_nano_usd,
        Some(1)
    );
}

use chrono::Utc;
use codex2api_storage::{
    ApiGroup, ApiListQuery, ApiModelRule, ApiPrincipal, ImagePrice, ModelConfig, ModelPrice,
    NewSupplierAccount, Storage, StorageError, UsageRecord, User, UserKind, WalletAdjustment,
};
use serde_json::json;

async fn setup() -> (
    codex2api_storage::test_support::TestDirectory,
    Storage,
    User,
    ApiGroup,
    String,
) {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("api-pg"))
        .await
        .unwrap();
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
        search_price_usd: None,
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
        status: "in_progress".into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn cached_catalog_is_fenced_by_transactional_price_and_model_revisions() {
    let (_dir, storage, user, group, supplier) = setup().await;
    let (secret, principal) = credential(&storage, &user, &group).await;
    storage
        .admit_api_request(&principal, &request("warm-cache"))
        .await
        .unwrap();
    // Simulate the commit/invalidation race: change PostgreSQL without invalidating L1/L2.
    sqlx::query("UPDATE model_prices SET input_rate=2000000 WHERE provider_id='chatgpt' AND model='api-text'")
        .execute(storage.pool()).await.unwrap();
    let mut current = request("new-price");
    storage
        .admit_api_request(&principal, &current)
        .await
        .unwrap();
    current.account_id = supplier;
    current.status = "completed".into();
    current.input_tokens = Some(1000);
    current.output_tokens = Some(0);
    assert_eq!(
        storage.settle(&current).await.unwrap().cost_nano_usd,
        Some(3_000_000)
    );
    sqlx::query(
        "UPDATE model_catalog SET enabled=FALSE WHERE provider_id='chatgpt' AND model='api-text'",
    )
    .execute(storage.pool())
    .await
    .unwrap();
    assert!(matches!(
        storage
            .admit_api_request(&principal, &request("disabled-model"))
            .await,
        Err(StorageError::Api {
            code: "model_not_available",
            ..
        })
    ));
    sqlx::query("UPDATE api_keys SET revoked_at='revoked' WHERE id=$1")
        .bind(&principal.key_id)
        .execute(storage.pool())
        .await
        .unwrap();
    assert!(
        storage
            .authenticate_api_key(&secret)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn wallet_deltas_use_current_balance_after_usage_and_debt_settlement() {
    let (_dir, storage, user, group, _) = setup().await;
    let snapshot = storage.user(&user.id).await.unwrap().unwrap();
    let admin = storage.require_admin_user().await.unwrap();
    let (_, principal) = credential(&storage, &user, &group).await;
    let adjustment = |id: &str, amount| WalletAdjustment {
        request_id: id.into(),
        amount_cents: amount,
        expected_revision: snapshot.revision,
        reason: None,
    };
    let mut usage = request("wallet-consumption");
    storage.admit_api_request(&principal, &usage).await.unwrap();
    usage.status = "completed".into();
    usage.input_tokens = Some(200_000);
    usage.cached_tokens = Some(0);
    usage.cache_write_tokens = Some(0);
    usage.output_tokens = Some(0);
    assert_eq!(storage.settle(&usage).await.unwrap().wallet_debit_cents, 30);
    let increase = storage
        .adjust_user_wallet(&user.id, admin.id, adjustment("increase-after-usage", 25))
        .await
        .unwrap();
    assert_eq!(
        (increase.balance_before_cents, increase.balance_cents),
        (70, 95)
    );
    let decrease = storage
        .adjust_user_wallet(&user.id, admin.id, adjustment("decrease-after-usage", -20))
        .await
        .unwrap();
    assert_eq!(
        (decrease.balance_before_cents, decrease.balance_cents),
        (95, 75)
    );
    let replay = storage
        .adjust_user_wallet(&user.id, admin.id, adjustment("increase-after-usage", 25))
        .await
        .unwrap();
    assert_eq!(replay.id, increase.id);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        75
    );

    let mut debt = request("wallet-debt");
    storage.admit_api_request(&principal, &debt).await.unwrap();
    debt.status = "completed".into();
    debt.input_tokens = Some(1_000_000);
    debt.cached_tokens = Some(0);
    debt.cache_write_tokens = Some(0);
    debt.output_tokens = Some(0);
    assert_eq!(storage.settle(&debt).await.unwrap().wallet_debit_cents, 75);
    let replenished = storage
        .adjust_user_wallet(&user.id, admin.id, adjustment("repay-debt", 100))
        .await
        .unwrap();
    assert_eq!(
        (replenished.balance_before_cents, replenished.balance_cents),
        (0, 100)
    );
    let current = storage.user(&user.id).await.unwrap().unwrap();
    assert_eq!(current.wallet_cents, 25);
    assert_eq!(current.revision, snapshot.revision);
    let entries: Vec<(i64, i64, i64)> = sqlx::query_as("SELECT amount_cents,balance_before_cents,balance_cents FROM wallet_entries WHERE user_id=$1 ORDER BY sort_id")
        .bind(&user.id).fetch_all(storage.pool()).await.unwrap();
    assert_eq!(
        entries,
        vec![
            (100, 0, 100),
            (-30, 100, 70),
            (25, 70, 95),
            (-20, 95, 75),
            (-75, 75, 0),
            (100, 0, 100),
            (-75, 100, 25)
        ]
    );
    storage.close().await;
}

#[tokio::test]
async fn api_keys_catalogs_and_queries_enforce_regular_ownership_and_secret_isolation() {
    let (_dir, storage, user, group, supplier) = setup().await;
    let (secret, principal) = credential(&storage, &user, &group).await;
    assert_eq!(secret.len(), 71);
    let (hash, prefix): (String, String) =
        sqlx::query_as("SELECT token_hash,key_prefix FROM api_keys WHERE id=$1")
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
                search: "api-".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(catalog["total"], 3);
    let text_model = catalog["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["model"] == "api-text")
        .unwrap();
    assert_eq!(text_model["prices"][0]["input_usd_per_million"], "1.5");
    assert_eq!(text_model["prices"][0]["base_input_usd_per_million"], "1");
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
    sqlx::query("INSERT INTO accounts(id,account_type,username,password_hash,enabled,created_at,updated_at) VALUES('virtual','user','virtual','hash',TRUE,'now','now')").execute(storage.pool()).await.unwrap();
    assert!(
        sqlx::query(
            "INSERT INTO users(id,name,email,kind) VALUES('virtual','Virtual','','virtual')"
        )
        .execute(storage.pool())
        .await
        .is_err()
    );
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
        sqlx::query("UPDATE api_keys SET user_id='another' WHERE id=$1")
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
        sqlx::query("UPDATE api_keys SET revoked_at=NULL,enabled=TRUE WHERE id=$1")
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
    storage.admit_api_request(&principal, &first).await.unwrap();
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
    let settlement = storage.settle(&first).await.unwrap();
    assert_eq!(settlement.cost_nano_usd, Some(9_675_000));
    assert_eq!(settlement.wallet_debit_cents, 0);
    let records = storage
        .query_usage(&Default::default())
        .await
        .unwrap()
        .records;
    let record = records.iter().find(|record| record.id == "first").unwrap();
    assert_eq!(record.source_name.as_deref(), Some("API group"));
    let calculation = serde_json::to_value(record.billing_calculation.as_ref().unwrap()).unwrap();
    assert_eq!(
        calculation["terms"],
        json!([
            {"kind":"cache_write","quantity":1000,"unit_price_usd":"1.25"},
            {"kind":"cache_read","quantity":2000,"unit_price_usd":"0.1"},
            {"kind":"output","quantity":1000,"unit_price_usd":"2"},
            {"kind":"input","quantity":3000,"unit_price_usd":"1"}
        ])
    );
    assert_eq!(calculation["total_nano_usd"], 9_675_000);
    assert_eq!(calculation["multiplier_micros"], 1_500_000);
    let cost = record.cost_calculation.as_ref().unwrap();
    assert_eq!(cost.total_nano_usd, 6_450_000);
    assert_eq!(cost.multiplier_micros, 1_000_000);
    let public = storage
        .user_store()
        .usage(&user.id, &Default::default())
        .await
        .unwrap();
    assert_eq!(public["items"][0]["billing_calculation"], calculation);
    for forbidden in [
        "cost_calculation",
        "base_cost_nano_usd",
        "pricing_snapshot_json",
    ] {
        assert!(public["items"][0].get(forbidden).is_none(), "{forbidden}");
    }
    let supplier_usage = storage
        .supplier_cycle_usage(&supplier, 0, Utc::now().timestamp_millis() + 1)
        .await
        .unwrap();
    assert_eq!(supplier_usage.request_count, 1);
    assert_eq!(supplier_usage.cost_nano_usd, Some(6_450_000));
    assert_eq!(storage.settle(&first).await.unwrap().wallet_debit_cents, 0);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        100
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT pending_nano_usd FROM usage_wallet_state WHERE user_id='regular'"
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        9_675_000
    );
    let mut stale = request("stale");
    assert!(matches!(
        storage.admit_api_request(&principal, &stale).await,
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
    storage.admit_api_request(&principal, &stale).await.unwrap();
    stale.input_tokens = Some(100);
    stale.output_tokens = Some(0);
    stale.status = "completed".into();
    let result = storage.settle(&stale).await.unwrap();
    assert_eq!(result.cost_nano_usd, Some(10_000_000));
    assert_eq!(result.wallet_debit_cents, 1);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        99
    );
    assert_eq!(storage.settle(&stale).await.unwrap().wallet_debit_cents, 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM wallet_entries WHERE kind='usage'")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        1
    );
    assert!(
        sqlx::query("UPDATE usage_records SET cost_nano_usd=0 WHERE id='second'")
            .execute(storage.pool())
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM usage_records WHERE id='second'")
            .execute(storage.pool())
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM usage_records")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        2
    );
    let usage = storage
        .user_store()
        .usage(&user.id, &Default::default())
        .await
        .unwrap();
    assert_eq!(usage["total"], 2);
    assert_eq!(usage["wallet_pending_nano_usd"], 9_675_000);
    assert!(!usage.to_string().contains("supplier_account_id"));
    let mut missing = request("missing-price");
    missing.model = Some("api-unpriced".into());
    assert!(matches!(
        storage.admit_api_request(&principal, &missing).await,
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
    let preview = storage
        .checkout_preview(
            &user.id,
            codex2api_storage::CheckoutInput {
                subscription_id: None,
                plan_id: plan.id,
                plan_revision: plan.revision,
                subscription_revision: None,
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
    let text_model = catalog
        .iter()
        .find(|model| model.model == "api-text")
        .unwrap();
    assert_eq!(text_model.multiplier, "2");
    assert_eq!(text_model.concurrency_limit, 2);
    assert_eq!(text_model.group_concurrency_limit, 5);
    let mut a = request("over-a");
    let mut b = request("over-b");
    storage.admit_api_request(&principal, &a).await.unwrap();
    storage.admit_api_request(&principal, &b).await.unwrap();
    for r in [&mut a, &mut b] {
        r.input_tokens = Some(400_000);
        r.output_tokens = Some(0);
        r.status = "completed".into();
    }
    let (a, b) = tokio::join!(storage.settle(&a), storage.settle(&b));
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
            "SELECT pending_nano_usd FROM usage_wallet_state WHERE user_id='regular'"
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        600_000_000
    );
    assert!(matches!(
        storage
            .admit_api_request(&principal, &request("after"))
            .await,
        Err(StorageError::Api {
            code: "insufficient_balance",
            ..
        })
    ));
    let unmatched = storage
        .user_store()
        .usage(
            &user.id,
            &codex2api_storage::UserUsageFilter {
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
                "SELECT pending_nano_usd FROM usage_wallet_state WHERE user_id='regular'"
            )
            .fetch_one(storage.pool())
            .await
            .unwrap(),
            pending
        );
    }
    let debt_entries:i64=sqlx::query_scalar("SELECT COUNT(*) FROM wallet_entries w JOIN usage_records r ON r.id=w.usage_record_id AND r.user_id=w.user_id WHERE w.request_id LIKE 'usage-debt:%' AND r.cost_nano_usd>0").fetch_one(storage.pool()).await.unwrap();
    assert_eq!(debt_entries, 2);
    storage
        .admit_api_request(&principal, &request("interrupted"))
        .await
        .unwrap();
    assert_eq!(storage.recover_interrupted_usage().await.unwrap(), 1);
    assert_eq!(storage.recover_interrupted_usage().await.unwrap(), 0);
    let interruption: (String, Option<i64>) = sqlx::query_as(
        "SELECT billing_status,cost_nano_usd FROM usage_records WHERE id='interrupted'",
    )
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(interruption, ("missing_usage".into(), None));
}

#[tokio::test]
async fn concurrent_completion_of_one_request_debits_the_wallet_once() {
    let (_dir, storage, user, group, _) = setup().await;
    let (_, principal) = credential(&storage, &user, &group).await;
    let mut record = request("same-completion");
    storage
        .admit_api_request(&principal, &record)
        .await
        .unwrap();
    record.input_tokens = Some(100_000);
    record.output_tokens = Some(0);
    record.status = "completed".into();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..16 {
        let storage = storage.clone();
        let record = record.clone();
        tasks.spawn(async move { storage.finish_usage(&record).await });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap().unwrap();
    }
    let (entries, debit): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),COALESCE(SUM(-amount_cents),0)::bigint FROM wallet_entries WHERE usage_record_id=$1",
    )
    .bind(&record.id).fetch_one(storage.pool()).await.unwrap();
    assert_eq!(entries, 1);
    assert!(debit > 0);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        100 - debit
    );
}

#[tokio::test]
async fn search_compact_and_input_count_share_usage_with_operation_specific_billing() {
    let (_dir, storage, user, mut group, _) = setup().await;
    group.model_rules = vec![ApiModelRule {
        model: "api-unpriced".into(),
        multiplier: Some("7".into()),
        concurrency_limit: 0,
        rpm_limit: 0,
    }];
    storage
        .save_api_group(&group, Some(group.revision))
        .await
        .unwrap();
    let mut group = storage.api_group(&group.id).await.unwrap().unwrap();
    let (_, mut principal) = credential(&storage, &user, &group).await;
    let mut search = request("search");
    search.endpoint = "/v1/alpha/search".into();
    search.model = Some("api-unpriced".into());
    assert!(matches!(
        storage.admit_api_request(&principal, &search).await,
        Err(StorageError::Api {
            code: "operation_price_unavailable",
            ..
        })
    ));
    let price = codex2api_storage::OperationPrice {
        provider_id: "chatgpt".into(),
        operation: "search".into(),
        price_nano_usd: Some(20_000_000),
        revision: 0,
    };
    assert!(storage.save_operation_price(&price, None).await.unwrap());
    // Global subscription prices never enable an API group without its own price.
    assert!(matches!(
        storage.admit_api_request(&principal, &search).await,
        Err(StorageError::Api {
            code: "operation_price_unavailable",
            ..
        })
    ));
    group.search_price_usd = Some("0.03".into());
    assert!(
        storage
            .save_api_group(&group, Some(group.revision))
            .await
            .unwrap()
    );
    group = storage.api_group(&group.id).await.unwrap().unwrap();
    principal.group = group.clone();
    let mut second = group.clone();
    second.id = "second-group".into();
    second.name = "Second group".into();
    second.search_price_usd = Some("0.07".into());
    assert!(storage.save_api_group(&second, None).await.unwrap());
    assert_eq!(
        storage.api_group_models(&second.id).await.unwrap()[0]
            .search_price
            .as_deref(),
        Some("0.07")
    );
    let mut grok = second.clone();
    grok.id = "grok-group".into();
    grok.provider_id = "grok".into();
    grok.tag_ids.clear();
    grok.model_rules.clear();
    assert!(storage.save_api_group(&grok, None).await.is_err());
    assert!(sqlx::query("INSERT INTO api_groups(id,name,provider_id,created_at,search_price_nano_usd) VALUES('invalid-grok','Grok','grok','2026-01-01',1)")
        .execute(storage.pool()).await.is_err());
    grok.search_price_usd = None;
    assert!(storage.save_api_group(&grok, None).await.unwrap());

    let catalog = storage.api_group_models(&group.id).await.unwrap();
    let unpriced = catalog.iter().find(|m| m.model == "api-unpriced").unwrap();
    assert!(unpriced.prices.is_empty());
    assert_eq!(unpriced.multiplier, "7");
    assert_eq!(unpriced.search_price.as_deref(), Some("0.03"));
    storage
        .admit_api_request(&principal, &search)
        .await
        .unwrap();
    group.search_price_usd = Some("0.5".into());
    assert!(
        storage
            .save_api_group(&group, Some(group.revision))
            .await
            .unwrap()
    );
    group = storage.api_group(&group.id).await.unwrap().unwrap();
    principal.group = group.clone();
    search.status = "completed".into();
    let settled = storage.settle(&search).await.unwrap();
    assert_eq!(settled.cost_nano_usd, Some(30_000_000));
    let records = storage
        .query_usage(&Default::default())
        .await
        .unwrap()
        .records;
    let saved = records.iter().find(|record| record.id == "search").unwrap();
    assert_eq!(saved.base_cost_nano_usd, Some(20_000_000));
    assert_eq!(
        saved.cost_calculation.as_ref().unwrap().total_nano_usd,
        20_000_000
    );
    assert_eq!(
        saved.billing_calculation.as_ref().unwrap().total_nano_usd,
        30_000_000
    );
    assert_eq!(settled.wallet_debit_cents, 3);
    assert_eq!(storage.settle(&search).await.unwrap().wallet_debit_cents, 3);

    let mut compact = request("compact");
    compact.endpoint = "/v1/responses/compact".into();
    storage
        .admit_api_request(&principal, &compact)
        .await
        .unwrap();
    compact.input_tokens = Some(1_000_000);
    compact.output_tokens = Some(0);
    compact.status = "completed".into();
    let settled = storage.settle(&compact).await.unwrap();
    assert_eq!(settled.cost_nano_usd, Some(1_500_000_000));
    assert_eq!(settled.wallet_debit_cents, 97);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        0
    );

    let mut count = request("count");
    count.endpoint = "/v1/responses/input_tokens".into();
    count.model = Some("api-unpriced".into());
    storage.admit_api_request(&principal, &count).await.unwrap();
    count.status = "completed".into();
    let settled = storage.settle(&count).await.unwrap();
    assert_eq!(settled.cost_nano_usd, Some(0));
    assert_eq!(settled.wallet_debit_cents, 0);
    let count_row: (String, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT billing_status,input_tokens,output_tokens FROM usage_records WHERE id='count'",
    )
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(count_row, ("not_charged".into(), None, None));
    search.id = "search-after-debt".into();
    assert!(matches!(
        storage.admit_api_request(&principal, &search).await,
        Err(StorageError::Api {
            code: "insufficient_balance",
            ..
        })
    ));
    group.search_price_usd = Some("0".into());
    assert!(
        storage
            .save_api_group(&group, Some(group.revision))
            .await
            .unwrap()
    );
    principal.group = storage.api_group(&group.id).await.unwrap().unwrap();
    search.id = "free-search".into();
    storage
        .admit_api_request(&principal, &search)
        .await
        .unwrap();
    let settled = storage.settle(&search).await.unwrap();
    assert_eq!(settled.cost_nano_usd, Some(0));
    assert_eq!(settled.wallet_debit_cents, 0);
    count.id = "count-outside-group".into();
    count.model = Some("api-other".into());
    group = storage.api_group(&group.id).await.unwrap().unwrap();
    group.allowed_models = vec!["api-text".into(), "api-unpriced".into()];
    storage
        .save_api_group(&group, Some(group.revision))
        .await
        .unwrap();
    principal.group = storage.api_group(&group.id).await.unwrap().unwrap();
    assert!(matches!(
        storage.admit_api_request(&principal, &count).await,
        Err(StorageError::Api {
            code: "model_not_available",
            ..
        })
    ));
    let totals: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),SUM(cost_nano_usd)::bigint FROM usage_records WHERE wallet_user_id=$1",
    )
    .bind(&user.id)
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(totals, (4, 1_530_000_000));
    let entries = storage.wallet_entries(&user.id).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, "system_adjustment");
    let wallet = storage
        .user_store()
        .wallet_entries(&user.id, &Default::default())
        .await
        .unwrap();
    assert_eq!(wallet.total, 1);
    assert_eq!(wallet.items[0]["kind"], "system_adjustment");
    let wallet = storage
        .admin_wallet_entries(&Default::default())
        .await
        .unwrap();
    assert_eq!(wallet["total"], 1);
    assert_eq!(wallet["items"][0]["kind"], "system_adjustment");
    assert!(
        storage
            .admin_wallet_entries(&codex2api_storage::WalletEntryFilter {
                kind: "usage".into(),
                ..Default::default()
            })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn api_model_membership_pagination_and_provider_boundaries_follow_current_configuration() {
    let (_dir, storage, user, mut group, _) = setup().await;
    let all = storage.api_group_models(&group.id).await.unwrap();
    assert!(all.iter().any(|model| model.model == "api-other"));
    assert!(all.iter().all(|model| model.provider_id == "chatgpt"));
    assert_eq!(
        storage.user_store().api_groups(&user.id).await.unwrap()[0].models_count,
        all.len() as i64
    );

    // Model access stays platform-wide even when the group has no supplier tags.
    group.tag_ids.clear();
    storage
        .save_api_group(&group, Some(group.revision))
        .await
        .unwrap();
    group = storage.api_group(&group.id).await.unwrap().unwrap();
    assert_eq!(
        storage
            .api_group_models(&group.id)
            .await
            .unwrap()
            .into_iter()
            .map(|model| model.model)
            .collect::<Vec<_>>(),
        all.iter()
            .map(|model| model.model.clone())
            .collect::<Vec<_>>()
    );
    let (_, principal) = credential(&storage, &user, &group).await;
    let mut untagged = request("untagged-model-admission");
    untagged.model = Some("api-other".into());
    storage
        .admit_api_request(&principal, &untagged)
        .await
        .unwrap();

    let new_model = ModelConfig {
        provider_id: "chatgpt".into(),
        model: "api-new".into(),
        kind: "text".into(),
        enabled: true,
        deleted: false,
        revision: 0,
    };
    let mut new_price = storage
        .model_prices("chatgpt")
        .await
        .unwrap()
        .into_iter()
        .find(|price| price.model == "api-text")
        .unwrap();
    new_price.model = new_model.model.clone();
    storage
        .save_model_config(&new_model, &[new_price.clone()], &[], None)
        .await
        .unwrap();
    assert!(
        storage
            .api_group_models(&group.id)
            .await
            .unwrap()
            .iter()
            .any(|model| model.model == new_model.model)
    );
    let foreign = ModelConfig {
        provider_id: "grok".into(),
        model: "api-grok-only".into(),
        ..new_model.clone()
    };
    let foreign_price = ModelPrice {
        provider_id: foreign.provider_id.clone(),
        model: foreign.model.clone(),
        ..new_price.clone()
    };
    storage
        .save_model_config(&foreign, &[foreign_price], &[], None)
        .await
        .unwrap();
    assert!(
        !storage
            .api_group_models(&group.id)
            .await
            .unwrap()
            .iter()
            .any(|model| model.model == foreign.model)
    );
    let mut rejected = request("cross-provider-model");
    rejected.model = Some(foreign.model);
    assert!(matches!(
        storage.admit_api_request(&principal, &rejected).await,
        Err(StorageError::Api {
            code: "model_not_available",
            ..
        })
    ));

    // A newly enabled model joins all-model groups without editing those groups.
    let mut request_new = request("new-catalog-model");
    request_new.model = Some(new_model.model.clone());
    storage
        .admit_api_request(&principal, &request_new)
        .await
        .unwrap();
    storage
        .set_model_enabled("chatgpt", &new_model.model, false, 1)
        .await
        .unwrap();
    assert!(
        !storage
            .api_group_models(&group.id)
            .await
            .unwrap()
            .iter()
            .any(|model| model.model == new_model.model)
    );
    request_new.id = "disabled-catalog-model".into();
    assert!(matches!(
        storage.admit_api_request(&principal, &request_new).await,
        Err(StorageError::Api {
            code: "model_not_available",
            ..
        })
    ));
    storage
        .delete_model_config("chatgpt", &new_model.model, 2)
        .await
        .unwrap();
    request_new.id = "deleted-catalog-model".into();
    assert!(matches!(
        storage.admit_api_request(&principal, &request_new).await,
        Err(StorageError::Api {
            code: "model_not_available",
            ..
        })
    ));

    group.tag_ids = vec!["pool".into()];
    group.allowed_models = vec!["api-other".into()];
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
    assert_eq!(market["items"][0]["model"], "api-other");
    group = storage.api_group(&group.id).await.unwrap().unwrap();
    let (_, selected_principal) = credential(&storage, &user, &group).await;
    untagged.id = "selected-untagged-model".into();
    storage
        .admit_api_request(&selected_principal, &untagged)
        .await
        .unwrap();
    assert!(matches!(
        storage
            .admit_api_request(&selected_principal, &request("outside-allowlist"))
            .await,
        Err(StorageError::Api {
            code: "model_not_available",
            ..
        })
    ));
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
            .query_usage(&codex2api_storage::UsageFilter {
                page: 99,
                ..Default::default()
            })
            .await
            .unwrap()
            .page,
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
        storage.admit_api_request(&principal, &image).await,
        Err(StorageError::Api {
            code: "model_price_unavailable",
            ..
        })
    ));
    image.image_size = Some("1024x1024".into());
    storage.admit_api_request(&principal, &image).await.unwrap();
    image.status = "completed".into();
    image.image_count = Some(2);
    image.image_usage_json = Some(json!([{"resolution":"1024x1024","count":2}]).to_string());
    let settled = storage.settle(&image).await.unwrap();
    assert_eq!(settled.cost_nano_usd, Some(60_000_000));
    assert_eq!(settled.wallet_debit_cents, 6);
    let mut unknown = request("unknown");
    storage
        .admit_api_request(&principal, &unknown)
        .await
        .unwrap();
    unknown.status = "completed".into();
    assert_eq!(storage.settle(&unknown).await.unwrap().cost_nano_usd, None);
    let mut failed = request("failed");
    storage
        .admit_api_request(&principal, &failed)
        .await
        .unwrap();
    failed.status = "failed".into();
    let failed = storage.settle(&failed).await.unwrap();
    assert_eq!(failed.cost_nano_usd, Some(0));
    assert_eq!(failed.billing_status, "not_charged");
    let usage = storage
        .user_store()
        .usage(&user.id, &Default::default())
        .await
        .unwrap();
    assert_eq!(usage["summary"]["cost_nano_usd"], 60_000_000);
    assert_eq!(usage["summary"]["unpriced_requests"], 1);
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
            .admit_api_request(&principal, &truncated)
            .await
            .unwrap();
        truncated.account_id = supplier.clone();
        truncated.http_status = status;
        truncated.first_token_ms = first_byte;
        truncated.status = "failed".into();
        let settled = storage.settle(&truncated).await.unwrap();
        assert_eq!(settled.cost_nano_usd, None, "{id}");
        assert_eq!(settled.billing_status, "missing_usage", "{id}");
    }
    let mut rejected = request("rejected");
    storage
        .admit_api_request(&principal, &rejected)
        .await
        .unwrap();
    rejected.account_id = supplier;
    rejected.http_status = Some(429);
    rejected.status = "failed".into();
    assert_eq!(
        storage.settle(&rejected).await.unwrap().billing_status,
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
    storage.admit_api_request(&principal, &tiny).await.unwrap();
    tiny.input_tokens = Some(1);
    tiny.output_tokens = Some(0);
    tiny.status = "completed".into();
    assert_eq!(storage.settle(&tiny).await.unwrap().cost_nano_usd, Some(1));
}

// Assert the persisted shared ledger after its normal finalization path.
trait SettleUsage {
    async fn settle(&self, record: &UsageRecord) -> codex2api_storage::Result<UsageRecord>;
}

#[tokio::test]
async fn all_request_sources_share_history_statistics_and_failure_details() {
    use codex2api_storage::{UsageFilter, UsageGroup};
    let (_dir, storage, user, group, supplier) = setup().await;
    let (_, principal) = credential(&storage, &user, &group).await;
    let subscription = storage
        .save_user_subscription(codex2api_storage::SubscriptionChange {
            subscription_id: None,
            reissue: false,
            user_id: &user.id,
            plan_id: "plus",
            expires_at: None,
            enabled: true,
            revision: None,
        })
        .await
        .unwrap();
    let mut oauth = request("oauth-shared");
    let original_plan: (String, String) = sqlx::query_as("SELECT p.id,p.name FROM regular_platforms a JOIN virtual_plans p ON p.id=a.plan_id WHERE a.id=$1")
        .bind(&subscription).fetch_one(storage.pool()).await.unwrap();
    oauth.subject_id = subscription;
    oauth.subject_name = "Subscription".into();
    oauth.account_id = supplier.clone();
    storage.insert_usage(&oauth).await.unwrap();
    sqlx::query("UPDATE virtual_plans SET name='Renamed after request' WHERE id=$1")
        .bind(&original_plan.0)
        .execute(storage.pool())
        .await
        .unwrap();
    oauth.input_tokens = Some(1000);
    oauth.output_tokens = Some(0);
    oauth.status = "completed".into();
    storage.finish_usage(&oauth).await.unwrap();
    let saved = storage.query_usage(&Default::default()).await.unwrap();
    assert_eq!(
        saved.records[0].source_name.as_deref(),
        Some(original_plan.1.as_str())
    );
    assert!(
        sqlx::query("UPDATE usage_records SET source_name='changed' WHERE id='oauth-shared'")
            .execute(storage.pool())
            .await
            .is_err()
    );

    let mut failed = request("api-shared");
    storage
        .admit_api_request(&principal, &failed)
        .await
        .unwrap();
    failed.account_id = supplier.clone();
    failed.account_name = "private-supplier-name".into();
    failed.status = "failed".into();
    failed.http_status = Some(200);
    failed.first_token_ms = Some(35);
    failed.total_ms = Some(1200);
    failed.error_code = Some("upstream_incomplete".into());
    failed.error_message = Some("private diagnostic for administrators".into());
    failed.upstream_request_id = Some("private-upstream-request".into());
    storage.finish_usage(&failed).await.unwrap();
    storage.finish_usage(&failed).await.unwrap();
    let filter = UsageFilter {
        user_id: Some(user.id.clone()),
        from_ms: Some(0),
        until_ms: Some(Utc::now().timestamp_millis() + 1),
        ..Default::default()
    };
    let page = storage.query_usage(&filter).await.unwrap();
    assert_eq!(page.total, 2);
    let row = page.records.iter().find(|r| r.id == failed.id).unwrap();
    assert_eq!(row.error_message, failed.error_message);
    assert_eq!(row.error_code, failed.error_code);
    assert_eq!(row.http_status, Some(200));
    assert_eq!(row.first_token_ms, Some(35));
    assert_eq!(row.total_ms, Some(1200));
    assert_eq!(row.key_id.as_deref(), Some(principal.key_id.as_str()));
    assert_eq!(row.billing_status, "missing_usage");
    for (source, expected_id, expected_name) in [
        (
            codex2api_storage::UsageSource::Api,
            failed.id.as_str(),
            "Primary key",
        ),
        (
            codex2api_storage::UsageSource::Subscription,
            oauth.id.as_str(),
            user.username.as_str(),
        ),
    ] {
        let page = storage
            .query_usage(&UsageFilter {
                source: Some(source),
                ..filter.clone()
            })
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.records[0].id, expected_id);
        assert_eq!(page.records[0].consumer_name, expected_name);
        let page = storage
            .user_store()
            .usage(
                &user.id,
                &codex2api_storage::UserUsageFilter {
                    source: Some(source),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["id"], expected_id);
        assert_eq!(page["items"][0]["consumer_name"], expected_name);
    }
    let stats = storage
        .usage_statistics(
            &UsageFilter {
                from_ms: Some(Utc::now().timestamp_millis() - 86400000),
                ..filter
            },
            UsageGroup::Subject,
            0,
        )
        .await
        .unwrap();
    assert_eq!(stats.summary.request_count, 2);
    assert_eq!(stats.summary.failed_requests, 1);
    assert_eq!(stats.rows.len(), 2);
    assert_eq!(
        storage
            .supplier_cycle_usage(&supplier, 0, Utc::now().timestamp_millis() + 1)
            .await
            .unwrap()
            .request_count,
        2
    );
    let public = storage
        .user_store()
        .usage(&user.id, &Default::default())
        .await
        .unwrap();
    assert_eq!(public["total"], 2);
    assert_eq!(public["summary"]["request_count"], 2);
    for private in [
        "account_id",
        "account_name",
        "error_message",
        "upstream_request_id",
        "private-supplier-name",
        "private diagnostic",
        "private-upstream-request",
    ] {
        assert!(!public.to_string().contains(private), "{private}");
    }
    let mut other = user.clone();
    other.id = "other-owner".into();
    other.username = "other-owner".into();
    storage.save_user(&other, None).await.unwrap();
    assert_eq!(
        storage
            .user_store()
            .usage(&other.id, &Default::default())
            .await
            .unwrap()["total"],
        0
    );
    let tables: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.tables WHERE table_schema=current_schema() AND table_name='api_usage_records'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(tables, 0);
}

#[tokio::test]
async fn shared_finalization_rolls_back_wallet_when_record_cannot_be_saved() {
    let (_dir, storage, user, group, _) = setup().await;
    let (_, principal) = credential(&storage, &user, &group).await;
    let mut request = request("rollback");
    storage
        .admit_api_request(&principal, &request)
        .await
        .unwrap();
    request.input_tokens = Some(100_000);
    request.output_tokens = Some(0);
    request.status = "completed".into();
    sqlx::raw_sql("CREATE FUNCTION reject_settlement_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='rollback' AND NEW.status='completed' THEN RAISE EXCEPTION 'fixture write failure'; END IF; RETURN NEW; END; $$; CREATE TRIGGER reject_settlement BEFORE UPDATE ON usage_records FOR EACH ROW EXECUTE FUNCTION reject_settlement_fn();").execute(storage.pool()).await.unwrap();
    assert!(storage.finish_usage(&request).await.is_err());
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        100
    );
    let state: (String, Option<i64>) =
        sqlx::query_as("SELECT status,cost_nano_usd FROM usage_records WHERE id='rollback'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(state, ("in_progress".into(), None));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM wallet_entries WHERE kind='usage'")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER reject_settlement ON usage_records")
        .execute(storage.pool())
        .await
        .unwrap();
    storage.finish_usage(&request).await.unwrap();
    storage.finish_usage(&request).await.unwrap();
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        85
    );
}
impl SettleUsage for Storage {
    async fn settle(&self, record: &UsageRecord) -> codex2api_storage::Result<UsageRecord> {
        self.finish_usage(record).await?;
        Ok(sqlx::query_as("SELECT * FROM usage_records WHERE id=$1")
            .bind(&record.id)
            .fetch_one(self.pool())
            .await?)
    }
}

#[tokio::test]
async fn market_ranges_and_tier_multipliers_match_actual_settlement() {
    use codex2api_storage::{UsageFilter, preset_model_prices};
    let (_dir, storage, user, group, supplier) = setup().await;
    let mut prices = preset_model_prices("chatgpt", "gpt-5.5").unwrap();
    for price in &mut prices {
        price.model = "tier-test".into();
    }
    let model = ModelConfig {
        provider_id: "chatgpt".into(),
        model: "tier-test".into(),
        kind: "text".into(),
        enabled: true,
        deleted: false,
        revision: 0,
    };
    storage
        .save_model_config(&model, &prices, &[], None)
        .await
        .unwrap();
    storage
        .add_supplier_model(&supplier, &model.model)
        .await
        .unwrap();
    for group_id in [String::new(), group.id.clone()] {
        let catalog = storage
            .user_store()
            .api_catalog(
                &user.id,
                &ApiListQuery {
                    group_id,
                    search: model.model.clone(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let rows = catalog["items"][0]["prices"].as_array().unwrap();
        for tier in ["standard", "flex"] {
            let short = rows
                .iter()
                .find(|p| p["tier"] == tier && p["min_input_tokens"] == 0)
                .unwrap();
            let long = rows
                .iter()
                .find(|p| p["tier"] == tier && p["min_input_tokens"] == 272001)
                .unwrap();
            assert_eq!(short["max_input_tokens"], 272000);
            assert!(long["max_input_tokens"].is_null());
        }
        let fast = rows.iter().find(|p| p["tier"] == "fast").unwrap();
        assert_eq!(fast["standard_multiplier"], "2.5");
        assert_eq!(fast["max_input_tokens"], 272000);
        assert!(
            rows.iter()
                .filter(|p| p["tier"] == "flex")
                .all(|p| p["standard_multiplier"] == "0.5")
        );
    }
    for (tier, input, amount) in [
        ("standard", 272000, Some(1_360_000_000)),
        ("standard", 272001, Some(2_720_010_000)),
        ("fast", 272000, Some(3_400_000_000)),
        ("fast", 272001, Some(6_800_025_000)),
        ("flex", 272000, Some(680_000_000)),
        ("flex", 272001, Some(1_360_005_000)),
    ] {
        let mut record = UsageRecord {
            id: format!("{tier}-{input}"),
            subject_id: "tier-test-owner".into(),
            model: Some(model.model.clone()),
            endpoint: "/v1/responses".into(),
            service_tier: Some(tier.into()),
            status: "in_progress".into(),
            ..Default::default()
        };
        storage.insert_usage(&record).await.unwrap();
        record.status = "completed".into();
        record.input_tokens = Some(input);
        record.output_tokens = Some(0);
        storage.finish_usage(&record).await.unwrap();
        let rows = storage
            .query_usage(&UsageFilter::default())
            .await
            .unwrap()
            .records;
        let saved = rows.iter().find(|r| r.id == record.id).unwrap();
        assert_eq!(saved.cost_nano_usd, amount, "{tier} {input}");
    }
    let stored = storage.model_prices("chatgpt").await.unwrap();
    assert!(
        stored
            .iter()
            .find(|p| p.model == model.model && p.tier == "standard" && p.min_input_tokens == 0)
            .unwrap()
            .max_input_tokens
            .is_none()
    );
    prices
        .iter_mut()
        .find(|p| p.tier == "fast")
        .unwrap()
        .output_rate = 123_000_000;
    storage
        .save_model_config(&model, &prices, &[], Some(1))
        .await
        .unwrap();
    let catalog = storage.api_group_models(&group.id).await.unwrap();
    assert!(
        catalog
            .iter()
            .find(|m| m.model == model.model)
            .unwrap()
            .prices
            .iter()
            .find(|p| p.tier == "fast")
            .unwrap()
            .standard_multiplier
            .is_none()
    );
}

#[tokio::test]
async fn market_other_billing_uses_base_or_group_search_price_without_multiplication() {
    let (_dir, storage, user, mut group, _supplier) = setup().await;
    storage
        .save_operation_price(
            &codex2api_storage::OperationPrice {
                provider_id: "chatgpt".into(),
                operation: "search".into(),
                price_nano_usd: Some(7_000_000),
                revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    let base = storage
        .user_store()
        .api_catalog(&user.id, &Default::default())
        .await
        .unwrap();
    assert_eq!(
        base["operations"],
        json!([{"provider_id":"chatgpt","operation":"search","price_usd":"0.007"}])
    );
    let query = ApiListQuery {
        group_id: group.id.clone(),
        ..Default::default()
    };
    let unset = storage
        .user_store()
        .api_catalog(&user.id, &query)
        .await
        .unwrap();
    assert!(unset["operations"][0]["price_usd"].is_null());
    group.search_price_usd = Some("0.03".into());
    group.multiplier = "9".into();
    storage
        .save_api_group(&group, Some(group.revision))
        .await
        .unwrap();
    let grouped = storage
        .user_store()
        .api_catalog(&user.id, &query)
        .await
        .unwrap();
    assert_eq!(grouped["operations"][0]["price_usd"], "0.03");
    let grok = storage
        .user_store()
        .api_catalog(
            &user.id,
            &ApiListQuery {
                provider_id: "grok".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(grok["operations"], json!([]));
}

#[tokio::test]
async fn metered_uniform_images_admit_and_charge_independent_of_resolution() {
    let (_dir, storage, user, group, supplier) = setup().await;
    let config = ModelConfig {
        provider_id: "chatgpt".into(),
        model: "api-uniform-image".into(),
        kind: "image".into(),
        enabled: true,
        deleted: false,
        revision: 0,
    };
    storage
        .save_model_config(
            &config,
            &[],
            &[ImagePrice {
                provider_id: config.provider_id.clone(),
                model: config.model.clone(),
                resolution: "*".into(),
                price_nano_usd: 20_000_000,
            }],
            None,
        )
        .await
        .unwrap();
    storage
        .add_supplier_model(&supplier, &config.model)
        .await
        .unwrap();
    let created = storage
        .user_store()
        .create_api_key(&user.id, "image", &group.id)
        .await
        .unwrap();
    let principal = storage
        .authenticate_api_key(&created.secret)
        .await
        .unwrap()
        .unwrap();
    let mut image = request("uniform-image");
    image.endpoint = "/v1/images/generations".into();
    image.model = Some(config.model.clone());
    image.image_size = Some("16384x8192".into());
    storage.admit_api_request(&principal, &image).await.unwrap();
    image.status = "completed".into();
    image.image_count = Some(2);
    image.image_usage_json = Some(json!([{"resolution":null,"count":2}]).to_string());
    let settled = storage.settle(&image).await.unwrap();
    assert_eq!(settled.wallet_debit_cents, 6);
}

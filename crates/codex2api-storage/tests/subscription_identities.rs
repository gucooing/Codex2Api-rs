use codex2api_storage::{Storage, SubscriptionChange, User, UserKind};
use serde_json::json;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::borrow::Cow;

async fn user(storage: &Storage) -> User {
    let user = User {
        id: "owner".into(),
        kind: UserKind::Regular,
        username: "owner".into(),
        password_hash: "fixture".into(),
        name: "Owner".into(),
        email: "owner@example.test".into(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_user(&user, None).await.unwrap();
    user
}
async fn grant(storage: &Storage, owner: &str) -> String {
    storage
        .save_user_subscription(SubscriptionChange {
            subscription_id: None,
            reissue: false,
            user_id: owner,
            plan_id: "plus",
            expires_at: None,
            enabled: true,
            revision: None,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn subscriptions_are_explicit_and_same_platform_limits_and_state_are_independent() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("subscriptions.sqlite"))
        .await
        .unwrap();
    let owner = user(&storage).await;
    assert!(
        storage
            .user_subscriptions(Some(&owner.id), true)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        storage
            .virtual_plans()
            .await
            .unwrap()
            .iter()
            .all(|plan| plan.plan_type != "free")
    );
    let first = grant(&storage, &owner.id).await;
    let second = grant(&storage, &owner.id).await;
    assert_ne!(first, second);
    let defaults = storage.virtual_rpm_limit(&first).await.unwrap();
    assert_eq!(
        (defaults.effective_rpm, defaults.effective_concurrency),
        (40, 10)
    );
    let visible = storage
        .user_store()
        .subscriptions(&owner.id, &Default::default())
        .await
        .unwrap();
    assert_eq!(visible.total, 2);
    assert!(
        visible
            .items
            .iter()
            .all(|item| item["rpm_limit"] == 40 && item["concurrency_limit"] == 10)
    );
    assert!(
        !serde_json::to_string(&visible)
            .unwrap()
            .contains("supplier_tag_id")
    );
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["rpm_limit"] = 2.into();
    plan.config["concurrency_limit"] = 1.into();
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    for id in [&first, &second] {
        assert!(
            storage
                .admit_virtual_request(id, true, 100_000)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            storage
                .admit_virtual_request(id, true, 100_000)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            storage
                .admit_virtual_request(id, true, 100_000)
                .await
                .unwrap(),
            Some(60)
        );
    }
    let permit = storage.acquire_consumer(&first).await.unwrap().unwrap();
    assert!(storage.acquire_consumer(&first).await.unwrap().is_none());
    let other = storage.acquire_consumer(&second).await.unwrap().unwrap();
    drop((permit, other));
    storage
        .save_virtual_request_limits(&second, Some(0), Some(3))
        .await
        .unwrap();
    assert!(
        storage
            .admit_virtual_request(&second, true, 100_000)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .virtual_rpm_limit(&first)
            .await
            .unwrap()
            .effective_concurrency,
        1
    );
    assert_eq!(
        storage
            .virtual_rpm_limit(&second)
            .await
            .unwrap()
            .effective_concurrency,
        3
    );
    storage
        .save_virtual_client_state(
            &first,
            "cloud_preferences",
            &json!({"branch_format":"first/{task_id}"}),
            None,
        )
        .await
        .unwrap();
    assert!(
        storage
            .virtual_client_state(&second, "cloud_preferences")
            .await
            .unwrap()
            .is_none()
    );
    let revision = storage.platform_revision(&first).await.unwrap();
    storage
        .save_user_subscription(SubscriptionChange {
            subscription_id: Some(&first),
            reissue: false,
            user_id: &owner.id,
            plan_id: "plus",
            expires_at: Some("2020-01-01T00:00:00Z"),
            enabled: true,
            revision: Some(revision),
        })
        .await
        .unwrap();
    assert!(
        !storage
            .effective_entitlements(&first)
            .await
            .unwrap()
            .execution_enabled
    );
    assert!(
        storage
            .effective_entitlements(&second)
            .await
            .unwrap()
            .execution_enabled
    );
    assert_eq!(
        storage
            .platform_account(&first)
            .await
            .unwrap()
            .unwrap()
            .plan_id,
        "plus"
    );
    assert_eq!(
        storage
            .oauth_subscriptions(&owner.id, "chatgpt")
            .await
            .unwrap()
            .len(),
        1
    );
    storage.delete_user_subscription(&first).await.unwrap();
    assert!(storage.user(&owner.id).await.unwrap().is_some());
    assert!(storage.platform_account(&second).await.unwrap().is_some());
    storage.close().await;
}

#[tokio::test]
async fn migration_removes_standalone_and_implicit_free_data_but_preserves_user_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("upgrade.sqlite");
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
                .filter(|m| m.version < 83)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
    .run(&pool)
    .await
    .unwrap();
    sqlx::raw_sql("INSERT INTO accounts(id,account_type,username,password_hash,enabled,created_at,updated_at) VALUES('owner','user','owner','hash',1,'now','now'),('free-owner','user','free-owner','hash',1,'now','now'),('standalone','user','standalone','hash',1,'now','now'),('operator','admin','operator','hash',1,'now','now');
      INSERT INTO users(id,name,email,wallet_cents,kind) VALUES('owner','Owner','o@example.test',1234,'regular'),('free-owner','Free','f@example.test',0,'regular'),('standalone','Removed','v@example.test',0,'virtual');
      INSERT INTO platform_accounts(id,user_id,provider_id,plan_id,plan_type,created_at) VALUES('paid','owner','chatgpt','plus','plus','2026-10-01'),('implicit','free-owner','chatgpt','free','free','2026-10-01'),('old-virtual','standalone','chatgpt','plus','plus','2026-10-01');
      INSERT INTO execution_routes(virtual_account_id,provider_id) VALUES('paid','chatgpt'),('implicit','chatgpt'),('old-virtual','chatgpt');
      INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision) VALUES('paid','cloud_preferences','{\"kept\":true}',1),('implicit','cloud_preferences','{}',1),('old-virtual','cloud_preferences','{\"deleted\":true}',1);
      INSERT INTO usage_records(id,account_id,account_name,subject_id,subject_name,user_id,endpoint,transport,requested_at_ms,status,cost_nano_usd) VALUES('kept-usage','','','paid','Owner','owner','/v1/responses','http',1,'completed',7),('old-free-usage','','','implicit','Free','free-owner','/v1/responses','http',1,'completed',8),('deleted-usage','','','old-virtual','Removed',NULL,'/v1/responses','http',1,'completed',9);
      INSERT INTO wallet_entries(id,user_id,request_id,signature,kind,amount_cents,balance_cents,created_at,operator_account_id,operator_name) VALUES('kept-wallet','owner','system:fixture','fixture','system_adjustment',1234,1234,'2026-10-01','operator','operator');
      INSERT INTO subscription_pricing_periods(id,subscription_id,plan_id,starts_at_ms,ends_at_ms,unit_price_cents,duration_days,source,created_at_ms) VALUES('kept-price','paid','plus',1,100,100,30,'admin',1),('old-price','implicit','plus',1,100,200,30,'admin',1),('deleted-price','old-virtual','plus',1,100,300,30,'admin',1);
      INSERT INTO meta(key,value) VALUES('gateway_settings','{\"default_rpm\":1,\"default_concurrency\":1}') ON CONFLICT(key) DO UPDATE SET value=excluded.value;")
      .execute(&pool).await.unwrap();
    pool.close().await;
    let storage = Storage::open(&path).await.unwrap();
    assert!(
        storage
            .user_by_username("standalone")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .platform_account("old-virtual")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .platform_account("implicit")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage.user("owner").await.unwrap().unwrap().wallet_cents,
        1234
    );
    assert_eq!(storage.wallet_entries("owner").await.unwrap().len(), 1);
    assert_eq!(
        storage
            .virtual_client_state("paid", "cloud_preferences")
            .await
            .unwrap()
            .unwrap()
            .value,
        json!({"kept":true})
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM usage_records")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        2
    );
    let periods: Vec<(String, String)> =
        sqlx::query_as("SELECT id,user_id FROM subscription_pricing_periods ORDER BY id")
            .fetch_all(storage.pool())
            .await
            .unwrap();
    assert_eq!(
        periods,
        vec![
            ("kept-price".into(), "owner".into()),
            ("old-price".into(), "free-owner".into())
        ]
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('platform_free_plans','virtual_users','virtual_platforms','admin_consumer_operations')").fetch_one(storage.pool()).await.unwrap(),0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    assert!(
        serde_json::to_value(storage.gateway_settings().await.unwrap())
            .unwrap()
            .get("default_rpm")
            .is_none()
    );
    let second = grant(&storage, "owner").await;
    assert_ne!(second, "paid");
    assert_eq!(
        storage
            .user_subscriptions(Some("owner"), true)
            .await
            .unwrap()
            .len(),
        2
    );
    storage.close().await;
}

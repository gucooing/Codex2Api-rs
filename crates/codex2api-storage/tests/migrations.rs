use codex2api_storage::Storage;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{borrow::Cow, path::Path};

#[tokio::test]
async fn removing_total_limit_preserves_windows_and_charges_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("remove-total-v22.sqlite");
    old_database(&path, 22, false).await;
    let pool = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    sqlx::query("INSERT INTO virtual_accounts(id,username,password_hash,name,email,plan_type,enabled,created_at) VALUES('v','virtual','hash','Virtual','v@example.test','pro',1,'2026-09-21')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision) VALUES('v','quota','{\"primary_cost_limit_usd\":2,\"weekly_cost_limit_usd\":10,\"total_cost_limit_usd\":0}',8)").execute(&pool).await.unwrap();
    sqlx::query("UPDATE usage_records SET api_key_id='v',cost_nano_usd=7000000000,billing_status='priced' WHERE id='test'").execute(&pool).await.unwrap();
    pool.close().await;
    let storage = Storage::open(&path).await.unwrap();
    let config = storage.virtual_config("v", "quota").await.unwrap();
    assert_eq!(
        config.value,
        serde_json::json!({"primary_cost_limit_usd":2,"weekly_cost_limit_usd":10})
    );
    assert_eq!(config.revision, 0);
    let quota = storage.virtual_quota("v").await.unwrap();
    assert_eq!(quota["rate_limit"]["allowed"], true);
    assert_eq!(quota["billing"]["used_usd"], "7");
    assert!(quota["billing"].get("limit_usd").is_none());
    assert_eq!(storage.model_prices("chatgpt").await.unwrap().len(), 42);
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    assert_eq!(
        reopened
            .virtual_config("v", "quota")
            .await
            .unwrap()
            .revision,
        0
    );
    assert_eq!(
        reopened.virtual_quota("v").await.unwrap()["rate_limit"]["allowed"],
        true
    );
}

static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");

#[tokio::test]
async fn gpt6_prices_seed_missing_models_without_overwriting_custom_or_deleted_catalog_entries() {
    for state in ["absent", "custom", "deleted"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prices.sqlite");
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        Migrator {
            migrations: Cow::Owned(
                MIGRATIONS
                    .iter()
                    .filter(|m| m.version <= 41)
                    .cloned()
                    .collect(),
            ),
            ..Migrator::DEFAULT
        }
        .run(&pool)
        .await
        .unwrap();
        if state != "absent" {
            sqlx::query("INSERT INTO model_catalog(provider_id,model,kind,enabled,deleted,revision) VALUES('chatgpt','gpt-6-sol','text',0,?,3)")
                .bind(i64::from(state=="deleted")).execute(&pool).await.unwrap();
        }
        if state == "custom" {
            sqlx::query("INSERT INTO model_prices(provider_id,model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('chatgpt','gpt-6-sol','standard',0,111,22,33,444,'custom')").execute(&pool).await.unwrap();
        }
        let before:Vec<(String,i64)>=sqlx::query_as("SELECT model,input_rate FROM model_prices WHERE model NOT IN ('gpt-6-sol','gpt-6-luna') ORDER BY model,tier,min_input_tokens").fetch_all(&pool).await.unwrap();
        pool.close().await;
        let storage = Storage::open(&path).await.unwrap();
        let prices = storage.model_prices("chatgpt").await.unwrap();
        let luna: Vec<_> = prices.iter().filter(|p| p.model == "gpt-6-luna").collect();
        assert_eq!(luna.len(), 6);
        assert_eq!(
            luna.iter()
                .find(|p| p.tier == "flex" && p.min_input_tokens == 0)
                .unwrap()
                .cache_write_rate,
            62500
        );
        let sol: Vec<_> = prices.iter().filter(|p| p.model == "gpt-6-sol").collect();
        match state {
            "absent" => assert_eq!(sol.len(), 6),
            "custom" => {
                assert_eq!(sol.len(), 1);
                assert_eq!(sol[0].input_rate, 111);
            }
            _ => assert!(sol.is_empty()),
        }
        let after:Vec<(String,i64)>=sqlx::query_as("SELECT model,input_rate FROM model_prices WHERE model NOT IN ('gpt-6-sol','gpt-6-luna') ORDER BY model,tier,min_input_tokens").fetch_all(storage.pool()).await.unwrap();
        assert_eq!(before, after);
        if state != "absent" {
            assert!(
                !storage
                    .model_config("chatgpt", "gpt-6-sol")
                    .await
                    .unwrap()
                    .unwrap()
                    .enabled
            );
        }
        storage.close().await;
        let storage = Storage::open(&path).await.unwrap();
        assert_eq!(
            storage.model_prices("chatgpt").await.unwrap().len(),
            prices.len()
        );
    }
}

#[tokio::test]
async fn gpt6_model_usage_bills_all_tiers_and_contexts_once_with_snapshot_prices() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("billing.sqlite"))
        .await
        .unwrap();
    let prices = storage.model_prices("chatgpt").await.unwrap();
    let mut expected = std::collections::BTreeMap::new();
    for price in prices
        .iter()
        .filter(|p| matches!(p.model.as_str(), "gpt-6-sol" | "gpt-6-luna"))
    {
        let input = if price.min_input_tokens == 0 {
            272000
        } else {
            272001
        };
        let id = format!("{}-{}-{}", price.model, price.tier, price.min_input_tokens);
        let mut row = codex2api_storage::UsageRecord {
            id: id.clone(),
            subject_id: "consumer".into(),
            model: Some(price.model.clone()),
            service_tier: Some(price.tier.clone()),
            endpoint: "/v1/responses".into(),
            status: "in_progress".into(),
            requested_at_ms: chrono::Utc::now().timestamp_millis(),
            ..Default::default()
        };
        storage.insert_usage(&row).await.unwrap();
        row.input_tokens = Some(input);
        row.output_tokens = Some(20000);
        row.cached_tokens = Some(50000);
        row.cache_write_tokens = Some(10000);
        row.reasoning_tokens = Some(5000);
        row.status = "completed".into();
        let nano = ((input - 60000) * price.input_rate
            + 50000 * price.cached_rate
            + 10000 * price.cache_write_rate
            + 20000 * price.output_rate
            + 500)
            / 1000;
        expected.insert(id, nano);
        // Editing prices after the request starts cannot alter this request.
        sqlx::query("UPDATE model_prices SET input_rate=input_rate+1 WHERE provider_id='chatgpt' AND model=? AND tier=? AND min_input_tokens=?")
            .bind(&price.model).bind(&price.tier).bind(price.min_input_tokens).execute(storage.pool()).await.unwrap();
        storage.finish_usage(&row).await.unwrap();
        storage.finish_usage(&row).await.unwrap();
    }
    let rows = storage.query_usage(&Default::default()).await.unwrap();
    assert_eq!(rows.total, 12);
    for row in rows.records {
        assert_eq!(row.cost_nano_usd, Some(expected[&row.id]));
        assert_eq!(row.billing_status, "priced");
    }
}

#[tokio::test]
async fn startup_preserves_release_lf_checksums_and_rejects_changed_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("release-line-endings.sqlite");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true),
        )
        .await
        .unwrap();
    // Release builds hash LF bytes. A Windows checkout containing CRLF used to
    // pass fresh-database tests but failed to open this same migration history.
    Migrator {
        migrations: Cow::Owned(
            MIGRATIONS
                .iter()
                .map(|migration| {
                    sqlx::migrate::Migration::new(
                        migration.version,
                        migration.description.clone(),
                        migration.migration_type,
                        Cow::Owned(migration.sql.replace("\r\n", "\n")),
                        migration.no_tx,
                    )
                })
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
    .run(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO meta(key,value) VALUES('migration_fixture','preserved')")
        .execute(&pool)
        .await
        .unwrap();
    let before: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    pool.close().await;

    let storage = Storage::open(&path)
        .await
        .expect("Windows builds must accept unchanged LF release migrations");
    let after: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(storage.pool())
            .await
            .unwrap();
    assert_eq!(after, before, "startup must not rewrite applied checksums");
    let marker: String = sqlx::query_scalar("SELECT value FROM meta WHERE key='migration_fixture'")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(marker, "preserved");

    // Only corrupt the disposable fixture. Real SQL edits must still be rejected.
    sqlx::query("UPDATE _sqlx_migrations SET checksum=? WHERE version=1")
        .bind(vec![0_u8; 48])
        .execute(storage.pool())
        .await
        .unwrap();
    storage.close().await;
    assert!(matches!(
        Storage::open(&path).await,
        Err(codex2api_storage::StorageError::Migration(
            sqlx::migrate::MigrateError::VersionMismatch(1)
        ))
    ));
}

#[tokio::test]
async fn supplier_routing_migration_preserves_credentials_and_marks_legacy_snapshots_unverified() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("routing-v38.sqlite");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true),
        )
        .await
        .unwrap();
    Migrator {
        migrations: Cow::Owned(
            MIGRATIONS
                .iter()
                .filter(|m| m.version <= 38)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
    .run(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO supplier_accounts(id,status,installation_id,originator,user_agent,os_type,os_version,arch,home_dir,created_at,updated_at) VALUES('legacy','active','keep-installation','codex_cli_rs','old-agent','Windows','11','x86_64','','2026-09-24','2026-09-24')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO supplier_tokens(account_id,access_token,refresh_token,updated_at) VALUES('legacy','old-access','keep-refresh','2026-09-24')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO supplier_info_cache(account_id,section,response_json,observed_at) VALUES('legacy','details','{\"accounts\":[]}','2026-09-24T00:00:00Z')").execute(&pool).await.unwrap();
    pool.close().await;
    let storage = Storage::open(&path).await.unwrap();
    assert_eq!(
        storage
            .require_account("legacy")
            .await
            .unwrap()
            .installation_id,
        "keep-installation"
    );
    let snapshot = storage
        .supplier_auth_snapshot("legacy")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.auth_revision, 0);
    assert_eq!(snapshot.tokens.access_token.as_deref(), Some("old-access"));
    let (details, revision) = storage
        .supplier_routing_snapshot("legacy")
        .await
        .unwrap()
        .unwrap();
    assert!(revision.is_none());
    assert!(
        storage
            .store_supplier_routing_snapshot("legacy", 0, &details)
            .await
            .unwrap()
    );
    let mut tokens = snapshot.tokens;
    tokens.access_token = Some("renewed-access".into());
    storage.upsert_supplier_tokens(tokens).await.unwrap();
    assert_eq!(
        storage.supplier_auth_revision("legacy").await.unwrap(),
        Some(1)
    );
    assert!(
        !storage
            .store_supplier_routing_snapshot("legacy", 0, &details)
            .await
            .unwrap()
    );
    assert_eq!(
        storage
            .load_supplier_tokens("legacy")
            .await
            .unwrap()
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("keep-refresh")
    );
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    assert_eq!(
        reopened.supplier_auth_revision("legacy").await.unwrap(),
        Some(1)
    );
}

#[tokio::test]
async fn plan_catalog_migration_preserves_effective_limits_policies_and_account_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog-v26.sqlite");
    old_database(&path, 26, false).await;
    let pool = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    for (id, kind) in [("a", "pro"), ("b", "pro"), ("c", "pro"), ("d", "free")] {
        sqlx::query("INSERT INTO virtual_accounts(id,username,password_hash,name,email,plan_type,subscription_expires_at,enabled,created_at) VALUES(?,?,'hash','Virtual','v@example.test',?,'2027-01-01T00:00:00Z',1,'2026-09-21')").bind(id).bind(id).bind(kind).execute(&pool).await.unwrap();
    }
    for id in ["a", "b", "c"] {
        let quota = if id == "c" {
            r#"{"primary_cost_limit_usd":1,"weekly_cost_limit_usd":null}"#
        } else {
            r#"{"primary_cost_limit_usd":2,"weekly_cost_limit_usd":10}"#
        };
        for (key, value) in [
            ("quota", quota),
            (
                "subscription_entitlements",
                r#"{"pro":{"models":["allowed"],"primary_cost_limit_usd":5,"weekly_cost_limit_usd":7}}"#,
            ),
            (
                "subscription_policy",
                r#"{"free_access_enabled":true,"primary_cost_limit_usd":0.25,"weekly_cost_limit_usd":1}"#,
            ),
        ] {
            sqlx::query("INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision) VALUES(?,?,?,3)").bind(id).bind(key).bind(value).execute(&pool).await.unwrap();
        }
    }
    pool.close().await;
    let storage = Storage::open(&path).await.unwrap();
    let a = storage.virtual_account("a").await.unwrap().unwrap();
    let b = storage.virtual_account("b").await.unwrap().unwrap();
    let c = storage.virtual_account("c").await.unwrap().unwrap();
    assert_eq!(a.plan_id, b.plan_id);
    assert_ne!(a.plan_id, c.plan_id);
    assert_eq!(
        a.subscription_expires_at.as_deref(),
        Some("2027-01-01T00:00:00Z")
    );
    assert_eq!(
        storage.virtual_config("a", "quota").await.unwrap().value,
        serde_json::json!({"primary_cost_limit_usd":2,"weekly_cost_limit_usd":7})
    );
    assert_eq!(
        storage.virtual_config("c", "quota").await.unwrap().value,
        serde_json::json!({"primary_cost_limit_usd":1,"weekly_cost_limit_usd":7})
    );
    assert!(
        storage
            .effective_entitlements("a")
            .await
            .unwrap()
            .models
            .permits("chatgpt", "allowed")
    );
    assert_eq!(
        storage
            .virtual_config("a", "subscription_policy")
            .await
            .unwrap()
            .value,
        serde_json::json!({"free_access_enabled":true,"primary_cost_limit_usd":0.25,"weekly_cost_limit_usd":1})
    );
    assert_eq!(
        storage.virtual_account("d").await.unwrap().unwrap().plan_id,
        "free"
    );
    assert_eq!(
        storage
            .virtual_client_state("a", "quota")
            .await
            .unwrap()
            .unwrap()
            .revision,
        3
    );
    let count = storage.virtual_plans().await.unwrap().len();
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    assert_eq!(reopened.virtual_plans().await.unwrap().len(), count);
    assert_eq!(
        reopened
            .virtual_account("a")
            .await
            .unwrap()
            .unwrap()
            .plan_id,
        a.plan_id
    );
    let usage: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM usage_records WHERE id='test'")
        .fetch_one(reopened.pool())
        .await
        .unwrap();
    assert_eq!(usage, 1);
}

async fn old_database(path: &Path, through: i64, drop_error_column: bool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await
        .unwrap();
    let migrator = Migrator {
        migrations: Cow::Owned(
            MIGRATIONS
                .iter()
                .filter(|m| m.version <= through)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    migrator.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO usage_records (id,account_id,account_name,api_key_id,api_key_name,endpoint,transport,model,reasoning_effort,input_tokens,output_tokens,cached_tokens,cache_write_tokens,reasoning_tokens,image_size,first_byte_ms,total_ms,requested_at_ms,status,http_status) VALUES ('test','account','SupplierAccount','key','Key','/v1/responses','http','model','high',123,45,67,8,9,'1024x1024',100,200,300,'completed',200)")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts (id,status,installation_id,originator,user_agent,os_type,os_version,arch,home_dir,created_at,updated_at) VALUES ('legacy-account','active','installation','codex_cli_rs','ua','Windows','10','x86_64','','2026-09-17','2026-09-17')")
        .execute(&pool).await.unwrap();
    if through < 7 {
        sqlx::query("INSERT INTO proxy_api_keys (id,account_id,key_hash,key_prefix,created_at,revoked_at) VALUES ('legacy-deleted','legacy-account','deleted-hash','c2a_deleted','2026-09-17','2026-09-17'), ('legacy-kept','legacy-account','kept-hash','c2a_kept','2026-09-17',NULL)")
            .execute(&pool).await.unwrap();
    } else {
        sqlx::query("INSERT INTO proxy_api_keys (id,account_id,key_hash,key_prefix,created_at) VALUES ('legacy-kept','legacy-account','kept-hash','c2a_kept','2026-09-17')")
            .execute(&pool).await.unwrap();
    }
    if drop_error_column {
        sqlx::query("ALTER TABLE usage_records DROP COLUMN error_message")
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;
}

async fn assert_final_schema(storage: &Storage) {
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('usage_records')")
            .fetch_all(storage.pool())
            .await
            .unwrap();
    for name in ["error_message", "error_code", "upstream_request_id"] {
        assert!(columns.iter().any(|column| column == name));
    }
    let version: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success=1")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(version, MIGRATIONS.iter().map(|m| m.version).max().unwrap());
    let virtual_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('virtual_accounts')")
            .fetch_all(storage.pool())
            .await
            .unwrap();
    assert!(!virtual_columns.iter().any(|name| matches!(
        name.as_str(),
        "sync_quota" | "primary_used_percent" | "weekly_used_percent"
    )));
    assert!(columns.iter().any(|s| s == "actual_model"));
    assert!(columns.iter().any(|s| s == "service_tier"));
    let key_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('proxy_api_keys')")
            .fetch_all(storage.pool())
            .await
            .unwrap();
    assert!(key_columns.is_empty());
    let quota_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('account_quota_cache')")
            .fetch_all(storage.pool())
            .await
            .unwrap();
    assert_eq!(
        quota_columns,
        ["account_id", "response_json", "observed_at"]
    );
    let indexes:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN ('usage_records_time','usage_records_account_time','usage_records_subject_time','usage_records_model_time')").fetch_one(storage.pool()).await.unwrap();
    assert_eq!(indexes, 4);
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
}

#[tokio::test]
async fn virtual_quota_removal_preserves_existing_accounts_devices_and_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("virtual-v17.sqlite");
    old_database(&path, 17, false).await;
    let pool = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    sqlx::query("INSERT INTO virtual_accounts(id,username,password_hash,name,email,plan_type,sync_quota,primary_used_percent,weekly_used_percent,created_at) VALUES('v','virtual','hash','Virtual','v@example.test','pro',1,95,99,'2026-09-01')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO virtual_devices(id,virtual_account_id,refresh_hash,user_agent,created_at,last_login_at) VALUES('device','v','refresh-hash','client','2026-09-01','2026-09-01')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision) VALUES('v','voice','{\"selected\":\"Cove\"}',3)").execute(&pool).await.unwrap();
    pool.close().await;
    let storage = Storage::open(&path).await.unwrap();
    assert_final_schema(&storage).await;
    assert_eq!(
        storage.virtual_account("v").await.unwrap().unwrap().name,
        "Virtual"
    );
    assert_eq!(storage.virtual_devices("v").await.unwrap()[0].id, "device");
    let voice = storage.virtual_config("v", "voice").await.unwrap();
    assert_eq!(voice.revision, 3);
    assert_eq!(voice.value["selected"], "Cove");
    assert!(storage.virtual_quota("v").await.unwrap()["rate_limit"]["primary_window"].is_null());
}

#[tokio::test]
async fn billing_migration_keeps_token_history_without_converting_tokens_to_dollars() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("billing-v20.sqlite");
    old_database(&path, 20, false).await;
    let pool = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    sqlx::query("INSERT INTO virtual_accounts(id,username,password_hash,name,email,plan_type,enabled,created_at) VALUES('v','virtual','hash','Virtual','v@example.test','pro',1,'2026-09-20')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision) VALUES('v','quota','{\"primary_token_limit\":100,\"weekly_token_limit\":100}',4)").execute(&pool).await.unwrap();
    pool.close().await;
    let storage = Storage::open(&path).await.unwrap();
    let old: String = sqlx::query_scalar(
        "SELECT value_json FROM virtual_client_state WHERE state_key='legacy_token_quota'",
    )
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&old).unwrap()["primary_token_limit"],
        100
    );
    let config = storage.virtual_config("v", "quota").await.unwrap();
    assert_eq!(
        config.value,
        serde_json::json!({"primary_cost_limit_usd":null,"weekly_cost_limit_usd":null})
    );
    let history:(i64,i64,String,Option<i64>)=sqlx::query_as("SELECT input_tokens,output_tokens,billing_status,cost_nano_usd FROM usage_records WHERE id='test'").fetch_one(storage.pool()).await.unwrap();
    assert_eq!(history, (123, 45, "legacy".into(), None));
    assert_eq!(storage.model_prices("chatgpt").await.unwrap().len(), 42);
    storage.close().await;
}

#[tokio::test]
async fn spending_window_migrations_remove_old_total_and_detect_stale_forms() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("windows-v21.sqlite");
    old_database(&path, 21, false).await;
    let pool = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    sqlx::query("INSERT INTO virtual_accounts(id,username,password_hash,name,email,plan_type,enabled,created_at) VALUES('v','virtual','hash','Virtual','v@example.test','pro',1,'2026-09-20')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision) VALUES('v','quota','{\"total_cost_limit_usd\":42.5}',4)").execute(&pool).await.unwrap();
    pool.close().await;
    let storage = Storage::open(&path).await.unwrap();
    let config = storage.virtual_config("v", "quota").await.unwrap();
    assert_eq!(
        config.value,
        serde_json::json!({"primary_cost_limit_usd":null,"weekly_cost_limit_usd":null})
    );
    assert_eq!(config.revision, 0);
    assert!(
        storage
            .update_virtual_config("v", "quota", &config.value, 4)
            .await
            .is_err()
    );
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    assert_eq!(
        reopened
            .virtual_config("v", "quota")
            .await
            .unwrap()
            .revision,
        0
    );
}

#[tokio::test]
async fn startup_migrates_fresh_v3_v4_and_manually_cleaned_v4_databases() {
    let temp = tempfile::tempdir().unwrap();
    for (name, version, drop_column) in [
        ("fresh", 0, false),
        ("v3", 3, false),
        ("v4", 4, false),
        ("v4_cleaned", 4, true),
        ("v4_missing_table", 4, false),
        ("v5", 5, false),
        ("v6", 6, false),
        ("v7", 7, false),
        ("v8", 8, false),
        ("v9", 9, false),
        ("v10", 10, false),
        ("v11", 11, false),
        ("v12", 12, false),
        ("v13", 13, false),
        ("v14", 14, false),
        ("v15", 15, false),
        ("v16", 16, false),
    ] {
        let path = temp.path().join(format!("{name}.sqlite"));
        if version > 0 {
            old_database(&path, version, drop_column).await;
        }
        if name == "v4_missing_table" {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(SqliteConnectOptions::new().filename(&path))
                .await
                .unwrap();
            sqlx::query("DROP TABLE usage_records")
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }
        let storage = Storage::open(&path).await.unwrap();
        assert_final_schema(&storage).await;
        if version > 0 && name != "v4_missing_table" {
            let rows = storage.query_usage(&Default::default()).await.unwrap();
            assert_eq!(rows.total, 1);
            let row = &rows.records[0];
            assert_eq!(row.id, "test");
            assert_eq!(row.account_id, "account");
            assert_eq!(row.account_name, "SupplierAccount");
            assert!(row.actual_model.is_none());
            assert_eq!(row.subject_id, "key");
            assert_eq!(row.subject_name, "Key");
            assert_eq!(row.reasoning_effort.as_deref(), Some("high"));
            assert!(row.service_tier.is_none());
            assert_eq!(row.input_tokens, Some(123));
            assert_eq!(row.output_tokens, Some(45));
            assert_eq!(row.cached_tokens, Some(67));
            assert_eq!(row.cache_write_tokens, Some(8));
            assert_eq!(row.reasoning_tokens, Some(9));
            assert_eq!(row.image_size.as_deref(), Some("1024x1024"));
            assert_eq!(row.first_byte_ms, Some(100));
            assert_eq!(row.total_ms, Some(200));
            assert_eq!(row.requested_at_ms, 300);
            assert_eq!(row.status, "completed");
            assert_eq!(row.http_status, Some(200));
        } else {
            assert_eq!(
                storage
                    .query_usage(&Default::default())
                    .await
                    .unwrap()
                    .total,
                0
            );
        }
        storage.close().await;
        // Restart must validate the same migration history and do no additional changes.
        let storage = Storage::open(&path).await.unwrap();
        assert_final_schema(&storage).await;
        storage.close().await;
    }
}

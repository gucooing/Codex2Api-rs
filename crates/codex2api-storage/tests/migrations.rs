use codex2api_storage::Storage;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{borrow::Cow, path::Path};

static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");

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
    sqlx::query("INSERT INTO usage_records (id,account_id,account_name,api_key_id,api_key_name,endpoint,transport,model,reasoning_effort,input_tokens,output_tokens,cached_tokens,cache_write_tokens,reasoning_tokens,image_size,first_byte_ms,total_ms,requested_at_ms,status,http_status) VALUES ('test','account','Account','key','Key','/v1/responses','http','model','high',123,45,67,8,9,'1024x1024',100,200,300,'completed',200)")
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
    assert!(!columns.iter().any(|s| s == "error_message"));
    let version: i64 =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success=1")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(version, 14);
    assert!(columns.iter().any(|s| s == "actual_model"));
    assert!(columns.iter().any(|s| s == "service_tier"));
    let key_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('proxy_api_keys')")
            .fetch_all(storage.pool())
            .await
            .unwrap();
    assert!(!key_columns.iter().any(|name| name == "revoked_at"));
    assert!(key_columns.iter().any(|name| name == "paused_at"));
    assert!(key_columns.iter().any(|name| name == "key_token"));
    let deleted: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM proxy_api_keys WHERE id='legacy-deleted'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(deleted, 0);
    let quota_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('account_quota_cache')")
            .fetch_all(storage.pool())
            .await
            .unwrap();
    assert_eq!(
        quota_columns,
        ["account_id", "response_json", "observed_at"]
    );
    let indexes:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN ('usage_records_time','usage_records_account_time','usage_records_key_time','usage_records_model_time')").fetch_one(storage.pool()).await.unwrap();
    assert_eq!(indexes, 4);
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(integrity, "ok");
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
        if version > 0 {
            let kept = storage.list_proxy_api_keys("legacy-account").await.unwrap();
            assert_eq!(kept.len(), 1);
            assert_eq!(kept[0].id, "legacy-kept");
            assert!(!kept[0].can_copy);
            assert!(kept[0].paused_at.is_none());
        }
        if version > 0 && name != "v4_missing_table" {
            let rows = storage.query_usage(&Default::default()).await.unwrap();
            assert_eq!(rows.total, 1);
            let row = &rows.records[0];
            assert_eq!(row.id, "test");
            assert_eq!(row.account_id, "account");
            assert_eq!(row.account_name, "Account");
            assert!(row.actual_model.is_none());
            assert_eq!(row.api_key_id, "key");
            assert_eq!(row.api_key_name, "Key");
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

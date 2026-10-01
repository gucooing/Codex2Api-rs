use codex2api_storage::Storage;
use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::borrow::Cow;

async fn columns(pool: &SqlitePool, table: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
        .bind(table)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn snapshot(pool: &SqlitePool, table: &str, columns: &[String]) -> Vec<String> {
    let values = columns
        .iter()
        .map(|name| format!("quote(\"{name}\")"))
        .collect::<Vec<_>>()
        .join(" || '|' || ");
    sqlx::query_scalar(&format!("SELECT {values} FROM {table} ORDER BY rowid"))
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn changing_supplier_uniqueness_preserves_every_related_row_and_enforces_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v44.sqlite");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .unwrap();
    let migrations = sqlx::migrate!("./migrations");
    Migrator {
        migrations: Cow::Owned(
            migrations
                .iter()
                .filter(|m| m.version <= 44)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    }
    .run(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO supplier_accounts(id,status,chatgpt_account_id,chatgpt_user_id,email,plan_type,installation_id,originator,user_agent,os_type,os_version,arch,home_dir,created_at,updated_at)
        VALUES('supplier','active','team','alice','alice@example.test','business','installation','codex_cli_rs','ua','Linux','6','aarch64','','created','updated');
        INSERT INTO supplier_tokens(account_id,access_token,refresh_token,updated_at) VALUES('supplier','access-fixture','refresh-fixture','tokens-updated');
        INSERT INTO supplier_runtime(account_id,session_id,extra_json,updated_at) VALUES('supplier','session','{}','runtime-updated');
        INSERT INTO account_quota_cache VALUES('supplier','{"rate_limit":{}}','quota-observed');
        INSERT INTO supplier_info_cache VALUES('supplier','usage','{"username":"Alice"}','info-observed',1);
        INSERT INTO supplier_health VALUES('supplier','existing error','error-at',5);
        INSERT INTO virtual_accounts(id,username,password_hash,name,email,plan_type,enabled,created_at,plan_id) VALUES('consumer','consumer','hash','Consumer','c@example.test','pro',1,'created','pro');
        INSERT INTO execution_routes VALUES('consumer','chatgpt','supplier',7);
        INSERT INTO usage_records(id,account_id,account_name,subject_id,subject_name,endpoint,transport,status,requested_at_ms) VALUES('usage','supplier','Alice','consumer','Consumer','/v1/responses','websocket','completed',1);
    "#).execute(&pool).await.unwrap();
    let tables = [
        "supplier_accounts",
        "supplier_tokens",
        "supplier_runtime",
        "account_quota_cache",
        "supplier_info_cache",
        "supplier_health",
        "execution_routes",
        "usage_records",
    ];
    let mut before = Vec::new();
    for table in tables {
        let columns = columns(&pool, table).await;
        let rows = snapshot(&pool, table, &columns).await;
        before.push((columns, rows));
    }
    // A caller bypassing Storage's migration connection must fail before a
    // parent rebuild can cascade into these child rows.
    assert!(migrations.run(&pool).await.is_err());
    for (table, (columns, expected)) in tables.iter().zip(&before) {
        assert_eq!(
            &snapshot(&pool, table, columns).await,
            expected,
            "failed migration: {table}"
        );
    }
    pool.close().await;
    let storage = Storage::open(&path).await.unwrap();
    for (table, (original_columns, expected)) in tables.iter().zip(&before) {
        // Later migrations may append fields; every pre-existing column and value must survive.
        let current_columns = columns(storage.pool(), table).await;
        assert!(
            original_columns
                .iter()
                .all(|column| current_columns.contains(column))
        );
        assert_eq!(
            &snapshot(storage.pool(), table, original_columns).await,
            expected,
            "{table}"
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pragma_foreign_key_check")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(storage.pool())
            .await
            .unwrap(),
        1
    );
    assert!(
        sqlx::query("INSERT INTO supplier_tokens(account_id,updated_at) VALUES('missing','now')")
            .execute(storage.pool())
            .await
            .is_err()
    );
    sqlx::query("UPDATE supplier_tokens SET access_token='rotated' WHERE account_id='supplier'")
        .execute(storage.pool())
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT auth_revision FROM supplier_accounts WHERE id='supplier'"
        )
        .fetch_one(storage.pool())
        .await
        .unwrap(),
        2
    );
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    assert_eq!(
        reopened
            .require_account("supplier")
            .await
            .unwrap()
            .installation_id,
        "installation"
    );
    reopened.delete_account("supplier").await.unwrap();
    assert!(
        reopened
            .load_supplier_tokens("supplier")
            .await
            .unwrap()
            .is_none()
    );
    let route = reopened
        .execution_route("consumer", "chatgpt")
        .await
        .unwrap()
        .unwrap();
    assert!(route.supplier_account_id.is_none());
    assert_eq!(route.revision, 8);
    assert_eq!(
        reopened
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        1
    );
    reopened.close().await;
}

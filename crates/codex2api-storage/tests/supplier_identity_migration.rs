use codex2api_storage::{NewSupplierAccount, Storage, SupplierStatus, SupplierTokens};
use serde_json::{Value, json};

async fn previous_schema(storage: &Storage) {
    sqlx::raw_sql("ALTER TABLE usage_records DROP COLUMN upstream_endpoint, DROP COLUMN client_ip; ALTER TABLE supplier_accounts ADD COLUMN originator TEXT NOT NULL DEFAULT '', ADD COLUMN user_agent TEXT NOT NULL DEFAULT ''; UPDATE meta SET value='1' WHERE key='schema_migration_version'; INSERT INTO meta(key,value) VALUES('codex_ref_commit','fixture'),('codex_ref_repo','fixture');")
        .execute(storage.pool()).await.unwrap();
}

async fn supplier(storage: &Storage, id: &str, provider: &str) -> Value {
    let ua = if provider == "chatgpt" {
        "codex_cli_rs/0.162.1 (Mac OS 15.6.0; aarch64) iTerm.app/3.5.11"
    } else {
        "grok-pager/1.0.45 grok-shell/1.0.45 (macos; aarch64)"
    };
    let originator = if provider == "chatgpt" {
        "codex_cli_rs"
    } else {
        "grok-pager"
    };
    let mut fingerprint = json!({"installation_id":id,"os_type":"Mac OS","os_version":"15.6.0","arch":"aarch64","originator":originator,"user_agent":ua,"timezone":"Asia/Taipei"});
    if provider == "chatgpt" {
        fingerprint["cookies"] = json!(["__cf_bm=fixture"]);
    } else {
        fingerprint["account_id"] = id.into();
    }
    let mut input = NewSupplierAccount::pending_identity(
        id,
        "Mac OS",
        "15.6.0",
        "aarch64",
        fingerprint.to_string(),
    );
    input.id = Some(id.into());
    input.provider_id = provider.into();
    input.chatgpt_account_id = Some(format!("workspace-{id}"));
    input.chatgpt_user_id = Some(format!("user-{id}"));
    storage
        .save_authorized_account(
            input,
            SupplierTokens {
                access_token: Some("fixture-access".into()),
                refresh_token: Some("fixture-refresh".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    storage
        .set_account_status(id, SupplierStatus::Disabled)
        .await
        .unwrap();
    sqlx::query("UPDATE supplier_accounts SET originator=$1,user_agent=$2 WHERE id=$3")
        .bind(originator)
        .bind(ua)
        .bind(id)
        .execute(storage.pool())
        .await
        .unwrap();
    fingerprint
}

#[tokio::test]
async fn migration_preserves_frozen_inputs_credentials_and_manual_disablement() {
    let directory = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(directory.path().join("identity"))
        .await
        .unwrap();
    previous_schema(&storage).await;
    let mut chatgpt = supplier(&storage, "chatgpt-fixture", "chatgpt").await;
    let mut grok = supplier(&storage, "grok-fixture", "grok").await;
    for fingerprint in [&mut chatgpt, &mut grok] {
        fingerprint.as_object_mut().unwrap().remove("originator");
        fingerprint.as_object_mut().unwrap().remove("user_agent");
    }
    chatgpt["terminal"] = "iTerm.app/3.5.11".into();
    let migrated = storage.reopen().await.unwrap();
    for (id, expected) in [("chatgpt-fixture", chatgpt), ("grok-fixture", grok)] {
        let account = migrated.require_account(id).await.unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&account.http_fingerprint_json).unwrap(),
            expected
        );
        assert_eq!(account.status, SupplierStatus::Disabled);
        assert_eq!(account.installation_id, id);
        assert_eq!(
            migrated
                .load_supplier_tokens(id)
                .await
                .unwrap()
                .unwrap()
                .refresh_token
                .as_deref(),
            Some("fixture-refresh")
        );
        assert_eq!(migrated.supplier_auth_revision(id).await.unwrap(), Some(1));
    }
    let columns: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='supplier_accounts' AND column_name IN ('originator','user_agent')")
        .fetch_one(migrated.pool()).await.unwrap();
    assert_eq!(columns, 0);
    let copied_constants: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM meta WHERE key IN ('codex_ref_commit','codex_ref_repo')",
    )
    .fetch_one(migrated.pool())
    .await
    .unwrap();
    assert_eq!(copied_constants, 0);
    let again = migrated.reopen().await.unwrap();
    assert_eq!(
        again
            .require_account("chatgpt-fixture")
            .await
            .unwrap()
            .http_fingerprint_json,
        migrated
            .require_account("chatgpt-fixture")
            .await
            .unwrap()
            .http_fingerprint_json
    );
    again.close().await;
    migrated.close().await;
    storage.close().await;
}

#[tokio::test]
async fn malformed_identity_rolls_back_data_schema_and_migration_version() {
    let directory = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(directory.path().join("invalid"))
        .await
        .unwrap();
    previous_schema(&storage).await;
    let good = supplier(&storage, "a-good", "chatgpt").await;
    supplier(&storage, "z-invalid", "chatgpt").await;
    sqlx::query("UPDATE supplier_accounts SET user_agent='missing terminal' WHERE id='z-invalid'")
        .execute(storage.pool())
        .await
        .unwrap();
    assert!(storage.reopen().await.is_err());
    let fingerprint: String =
        sqlx::query_scalar("SELECT http_fingerprint_json FROM supplier_accounts WHERE id='a-good'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&fingerprint).unwrap(), good);
    let version: String =
        sqlx::query_scalar("SELECT value FROM meta WHERE key='schema_migration_version'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(version, "1");
    let old_ua: String =
        sqlx::query_scalar("SELECT user_agent FROM supplier_accounts WHERE id='a-good'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    assert_eq!(old_ua, good["user_agent"].as_str().unwrap());
    storage.close().await;
}

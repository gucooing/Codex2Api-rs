use codex2api_storage::{NewSupplierAccount, Storage, SupplierTokens};

#[tokio::test]
async fn authorized_account_and_credentials_commit_together() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authorized.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let mut account = NewSupplierAccount::pending_identity(
        "installation",
        "codex_cli_rs",
        "ua",
        "Linux",
        "6",
        "aarch64",
        "",
        "{}",
    );
    account.id = Some("draft".into());
    account.chatgpt_account_id = Some("chatgpt-account".into());
    let tokens = SupplierTokens {
        account_id: "draft".into(),
        access_token: Some("access-fixture".into()),
        refresh_token: Some("refresh-fixture".into()),
        ..Default::default()
    };
    sqlx::query("CREATE TRIGGER reject_tokens BEFORE INSERT ON supplier_tokens BEGIN SELECT RAISE(ABORT, 'test failure'); END")
        .execute(storage.pool()).await.unwrap();
    assert!(
        storage
            .save_authorized_account(account.clone(), tokens.clone(), None)
            .await
            .is_err()
    );
    assert!(storage.list_accounts().await.unwrap().is_empty());
    assert!(
        storage
            .load_supplier_tokens("draft")
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("DROP TRIGGER reject_tokens")
        .execute(storage.pool())
        .await
        .unwrap();
    let saved = storage
        .save_authorized_account(account, tokens, None)
        .await
        .unwrap();
    assert_eq!(saved.status, codex2api_storage::SupplierStatus::Active);
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    assert_eq!(storage.list_accounts().await.unwrap().len(), 1);
    assert_eq!(
        storage
            .require_account("draft")
            .await
            .unwrap()
            .installation_id,
        "installation"
    );
    assert_eq!(
        storage
            .load_supplier_tokens("draft")
            .await
            .unwrap()
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("refresh-fixture")
    );
    storage.close().await;
}

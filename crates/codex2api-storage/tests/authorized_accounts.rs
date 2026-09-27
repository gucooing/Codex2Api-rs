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
    account.chatgpt_user_id = Some("chatgpt-user".into());
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

#[tokio::test]
async fn concurrent_logins_deduplicate_only_the_same_user_and_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("concurrent.sqlite"))
        .await
        .unwrap();
    let make = |id: &str, user: &str, workspace: &str| {
        let mut account = NewSupplierAccount::pending_identity(
            id,
            "codex_cli_rs",
            "ua",
            "Linux",
            "6",
            "aarch64",
            "",
            "{}",
        );
        account.id = Some(id.into());
        account.chatgpt_user_id = Some(user.into());
        account.chatgpt_account_id = Some(workspace.into());
        let tokens = SupplierTokens {
            account_id: id.into(),
            access_token: Some(format!("token-{user}-{workspace}")),
            ..Default::default()
        };
        (account, tokens)
    };
    let (a, at) = make("a", "alice", "team");
    let (b, bt) = make("b", "bob", "team");
    let (c, ct) = make("c", "alice", "personal");
    let (first, second, personal) = tokio::join!(
        storage.save_authorized_account(a, at, None),
        storage.save_authorized_account(b, bt, None),
        storage.save_authorized_account(c, ct, None)
    );
    assert_eq!(first.unwrap().id, "a");
    assert_eq!(second.unwrap().id, "b");
    assert_eq!(personal.unwrap().id, "c");
    let (a, at) = make("a-again", "alice", "team");
    let (b, bt) = make("a-again-concurrent", "alice", "team");
    let (a, b) = tokio::join!(
        storage.save_authorized_account(a, at, None),
        storage.save_authorized_account(b, bt, None)
    );
    for result in [a, b] {
        let account = result.unwrap();
        assert_eq!(account.id, "a");
        assert_eq!(account.installation_id, "a");
    }
    assert_eq!(storage.list_accounts().await.unwrap().len(), 3);
    for (id, user, workspace) in [
        ("a", "alice", "team"),
        ("b", "bob", "team"),
        ("c", "alice", "personal"),
    ] {
        assert_eq!(
            storage
                .load_supplier_tokens(id)
                .await
                .unwrap()
                .unwrap()
                .access_token,
            Some(format!("token-{user}-{workspace}"))
        );
    }
    let (mut unknown, tokens) = make("unknown", "", "team");
    unknown.chatgpt_user_id = None;
    assert!(
        storage
            .save_authorized_account(unknown, tokens, None)
            .await
            .is_err()
    );
    assert_eq!(storage.list_accounts().await.unwrap().len(), 3);
    storage.close().await;
}

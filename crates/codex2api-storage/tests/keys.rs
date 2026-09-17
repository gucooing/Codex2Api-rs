use codex2api_storage::{AccountStatus, NewAccount, Storage};

#[tokio::test]
async fn changing_binding_preserves_key_data_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("binding.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let mut accounts = Vec::new();
    for _ in 0..3 {
        accounts.push(
            storage
                .create_account(NewAccount::pending_identity(
                    uuid::Uuid::new_v4().to_string(),
                    "codex_cli_rs",
                    "ua",
                    "Windows",
                    "10",
                    "x86_64",
                    "",
                    "{}",
                ))
                .await
                .unwrap(),
        );
    }
    storage
        .set_account_status(&accounts[0].id, AccountStatus::Active)
        .await
        .unwrap();
    storage
        .set_account_status(&accounts[1].id, AccountStatus::Disabled)
        .await
        .unwrap();
    let key = storage
        .create_proxy_api_key(&accounts[0].id, Some("unchanged"))
        .await
        .unwrap();
    storage
        .set_proxy_api_key_paused(&accounts[0].id, &key.record.id, true)
        .await
        .unwrap();
    let before = storage
        .get_proxy_api_key(&key.record.id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !storage
            .bind_proxy_api_key(&key.record.id, "missing")
            .await
            .unwrap()
    );
    assert!(
        !storage
            .bind_proxy_api_key(&key.record.id, &accounts[2].id)
            .await
            .unwrap()
    );
    assert_eq!(
        storage
            .get_proxy_api_key(&key.record.id)
            .await
            .unwrap()
            .unwrap()
            .account_id,
        accounts[0].id
    );
    assert!(
        storage
            .bind_proxy_api_key(&key.record.id, &accounts[1].id)
            .await
            .unwrap()
    );
    storage.close().await;
    let storage = Storage::open(path).await.unwrap();
    let after = storage
        .get_proxy_api_key(&key.record.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.account_id, accounts[1].id);
    assert_eq!(after.key_hash, before.key_hash);
    assert_eq!(after.key_prefix, before.key_prefix);
    assert_eq!(after.name, before.name);
    assert_eq!(after.paused_at, before.paused_at);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(
        storage
            .proxy_api_key_token(&accounts[1].id, &key.record.id)
            .await
            .unwrap(),
        Some(key.token)
    );
    storage.close().await;
}

#[tokio::test]
async fn pause_copy_and_physical_delete_are_account_scoped_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keys.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let account = storage
        .create_account(NewAccount::pending_identity(
            uuid::Uuid::new_v4().to_string(),
            "codex_cli_rs",
            "ua",
            "Windows",
            "10",
            "x86_64",
            "",
            "{}",
        ))
        .await
        .unwrap();
    let key = storage
        .create_proxy_api_key(&account.id, Some("test"))
        .await
        .unwrap();
    assert!(key.record.can_copy);
    assert!(
        storage
            .lookup_proxy_api_key(&key.token)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        storage
            .proxy_api_key_token(&account.id, &key.record.id)
            .await
            .unwrap(),
        Some(key.token.clone())
    );
    assert!(
        storage
            .proxy_api_key_token("other", &key.record.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !storage
            .delete_proxy_api_key("other", &key.record.id)
            .await
            .unwrap()
    );
    assert!(
        !storage
            .set_proxy_api_key_paused("other", &key.record.id, true)
            .await
            .unwrap()
    );
    assert!(
        storage
            .set_proxy_api_key_paused(&account.id, &key.record.id, true)
            .await
            .unwrap()
    );
    assert!(
        storage
            .lookup_proxy_api_key(&key.token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage.list_proxy_api_keys(&account.id).await.unwrap()[0]
            .paused_at
            .is_some()
    );
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    assert!(
        storage
            .lookup_proxy_api_key(&key.token)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .proxy_api_key_token(&account.id, &key.record.id)
            .await
            .unwrap(),
        Some(key.token.clone())
    );
    assert!(
        storage
            .set_proxy_api_key_paused(&account.id, &key.record.id, false)
            .await
            .unwrap()
    );
    assert!(
        storage
            .lookup_proxy_api_key(&key.token)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        storage
            .delete_proxy_api_key(&account.id, &key.record.id)
            .await
            .unwrap()
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM proxy_api_keys WHERE id = ?")
        .bind(&key.record.id)
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        storage
            .lookup_proxy_api_key(&key.token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .proxy_api_key_token(&account.id, &key.record.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .list_proxy_api_keys(&account.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !storage
            .set_proxy_api_key_paused(&account.id, &key.record.id, false)
            .await
            .unwrap()
    );
    storage.close().await;
}

use codex2api_storage::{AccountStatus, AccountTokens, NewAccount, Storage, hash_api_key};

async fn account(storage: &Storage, id: &str) {
    let mut account = NewAccount::pending_identity(
        format!("install-{id}"),
        "codex_cli_rs",
        "ua",
        "Linux",
        "6",
        "aarch64",
        "",
        "{}",
    );
    account.id = Some(id.into());
    account.chatgpt_account_id = Some(format!("chatgpt-{id}"));
    storage
        .save_authorized_account(
            account,
            AccountTokens {
                account_id: id.into(),
                access_token: Some(format!("official-access-{id}")),
                refresh_token: Some(format!("official-refresh-{id}")),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn account_details_support_multiple_rts_devices_and_separate_login_from_usage() {
    use codex2api_storage::{OAuthDeviceIdentity, UsageFilter, UsageRecord};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("devices.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    account(&storage, "owner").await;
    account(&storage, "other").await;
    let first = storage
        .create_oauth_credential("owner", "Desktop RT")
        .await
        .unwrap();
    let second = storage
        .create_oauth_credential("owner", "Laptop RT")
        .await
        .unwrap();
    let other = storage
        .create_oauth_credential("other", "Other RT")
        .await
        .unwrap();
    let first_rt = storage
        .oauth_refresh_token(&first.id)
        .await
        .unwrap()
        .unwrap();
    let second_rt = storage
        .oauth_refresh_token(&second.id)
        .await
        .unwrap()
        .unwrap();
    let other_rt = storage
        .oauth_refresh_token(&other.id)
        .await
        .unwrap()
        .unwrap();
    let expires = chrono::Utc::now().timestamp() + 3600;
    let desktop = OAuthDeviceIdentity::new(Some("desktop"), "Client/1.0");
    let desktop_update = OAuthDeviceIdentity::new(Some("desktop"), "Client/2.0");
    let laptop = OAuthDeviceIdentity::new(Some("laptop"), "Client/1.0");
    for (credential, rt, access, device) in [
        (&first, first_rt.as_str(), "desktop-access", &desktop),
        (
            &first,
            first_rt.as_str(),
            "desktop-refresh",
            &desktop_update,
        ),
        (&first, first_rt.as_str(), "laptop-access", &laptop),
        (&second, second_rt.as_str(), "second-desktop", &desktop),
        (&other, other_rt.as_str(), "other-access", &desktop),
    ] {
        assert!(
            storage
                .register_oauth_access(credential, rt, access, expires, device)
                .await
                .unwrap()
        );
    }
    let devices = storage.oauth_devices_for_account("owner").await.unwrap();
    assert_eq!(devices.len(), 3);
    assert!(devices.iter().all(|d| d.last_used_at.is_none()));
    assert_eq!(devices.iter().find(|d|d.credential_id==first.id && d.installation_id.as_deref()==Some("desktop")).unwrap().user_agent,"Client/2.0");
    let summaries = storage.oauth_account_summaries().await.unwrap();
    let owner = summaries.iter().find(|s| s.account_id == "owner").unwrap();
    assert_eq!(owner.rt_count, 2);
    assert_eq!(owner.device_count, 2);
    assert!(owner.last_used_at.is_none());
    storage
        .touch_oauth_access(&hash_api_key("desktop-access"))
        .await
        .unwrap();
    let devices = storage.oauth_devices_for_account("owner").await.unwrap();
    assert_eq!(
        devices.iter().filter(|d| d.last_used_at.is_some()).count(),
        1
    );
    let used = devices.iter().find(|d| d.last_used_at.is_some()).unwrap();
    assert_eq!(used.credential_id, first.id);
    assert_eq!(used.installation_id.as_deref(), Some("desktop"));
    assert!(
        storage
            .get_oauth_credential(&second.id)
            .await
            .unwrap()
            .unwrap()
            .last_used_at
            .is_none()
    );
    assert!(
        storage.oauth_devices_for_account("other").await.unwrap()[0]
            .last_used_at
            .is_none()
    );

    let key = storage
        .create_proxy_api_key("owner", Some("API client"))
        .await
        .unwrap();
    let options = storage.usage_key_options().await.unwrap();
    assert_eq!(
        options.iter().find(|o| o.id == first.id).unwrap().name,
        "Desktop RT"
    );
    assert_eq!(
        options.iter().find(|o| o.id == key.record.id).unwrap().name,
        "API client"
    );
    for (id, source, name) in [
        ("oauth-record", first.id.as_str(), "Desktop RT"),
        ("key-record", key.record.id.as_str(), "API client"),
    ] {
        storage
            .insert_usage(&UsageRecord {
                id: id.into(),
                account_id: "owner".into(),
                api_key_id: source.into(),
                api_key_name: name.into(),
                requested_at_ms: 1,
                status: "completed".into(),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    let history = storage
        .query_usage(&UsageFilter {
            api_key_id: Some(first.id.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(history.total, 1);
    assert_eq!(history.records[0].api_key_name, "Desktop RT");
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    assert_eq!(
        storage
            .oauth_devices_for_account("owner")
            .await
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        storage
            .oauth_credentials_for_account("owner")
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(
        storage
            .oauth_account_summaries()
            .await
            .unwrap()
            .iter()
            .find(|s| s.account_id == "owner")
            .unwrap()
            .last_used_at
            .is_some()
    );
    storage.close().await;
}

#[tokio::test]
async fn credentials_persist_and_revocation_expiry_and_account_state_are_enforced() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("oauth.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    account(&storage, "first").await;
    account(&storage, "second").await;
    let first = storage
        .create_oauth_credential("first", "client-one")
        .await
        .unwrap();
    let second = storage
        .create_oauth_credential("second", "client-two")
        .await
        .unwrap();
    let rt = storage
        .oauth_refresh_token(&first.id)
        .await
        .unwrap()
        .unwrap();
    assert!(rt.starts_with("c2rt_"));
    let key = storage.oauth_signing_key().await.unwrap();
    let expires = chrono::Utc::now().timestamp() + 3600;
    assert!(
        storage
            .register_oauth_access(&first, &rt, "issued-access", expires, &Default::default())
            .await
            .unwrap()
    );
    assert!(
        !storage
            .register_oauth_access(&second, &rt, "wrong-binding", expires, &Default::default())
            .await
            .unwrap()
    );
    assert!(
        storage
            .register_oauth_access(&first, &rt, "expired-access", 1, &Default::default())
            .await
            .unwrap()
    );
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key("expired-access"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key("tampered-access"))
            .await
            .unwrap()
            .is_none()
    );
    let access = storage
        .lookup_oauth_access_hash(&hash_api_key("issued-access"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(access.account_id, "first");
    assert_eq!(access.credential_id, first.id);
    assert_eq!(storage.usage_key_options().await.unwrap().len(), 2);
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    assert_eq!(storage.oauth_signing_key().await.unwrap(), key);
    assert_eq!(
        storage
            .lookup_oauth_refresh(&rt)
            .await
            .unwrap()
            .unwrap()
            .account_id,
        "first"
    );
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key("issued-access"))
            .await
            .unwrap()
            .is_some()
    );
    storage
        .set_account_status("first", AccountStatus::Disabled)
        .await
        .unwrap();
    assert!(storage.lookup_oauth_refresh(&rt).await.unwrap().is_none());
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key("issued-access"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .create_oauth_credential("first", "disabled")
            .await
            .is_err()
    );
    storage
        .set_account_status("first", AccountStatus::Active)
        .await
        .unwrap();
    storage.set_oauth_paused(&first.id, true).await.unwrap();
    assert!(storage.lookup_oauth_refresh(&rt).await.unwrap().is_none());
    assert!(
        !storage
            .register_oauth_access(&first, &rt, "raced-access", expires, &Default::default())
            .await
            .unwrap()
    );
    storage.set_oauth_paused(&first.id, false).await.unwrap();
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key("issued-access"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .register_oauth_access(&first, &rt, "after-enable", expires, &Default::default())
            .await
            .unwrap()
    );
    storage.revoke_oauth_token("after-enable").await.unwrap();
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key("after-enable"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(storage.lookup_oauth_refresh(&rt).await.unwrap().is_some());
    storage.revoke_oauth_token(&rt).await.unwrap();
    assert!(storage.lookup_oauth_refresh(&rt).await.unwrap().is_none());
    assert_eq!(storage.list_oauth_credentials().await.unwrap().len(), 1);
    assert_eq!(
        storage
            .load_account_tokens("first")
            .await
            .unwrap()
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("official-refresh-first")
    );
    let second_rt = storage
        .oauth_refresh_token(&second.id)
        .await
        .unwrap()
        .unwrap();
    storage
        .register_oauth_access(
            &second,
            &second_rt,
            "second-access",
            expires,
            &Default::default(),
        )
        .await
        .unwrap();
    storage.delete_account("second").await.unwrap();
    assert!(storage.list_oauth_credentials().await.unwrap().is_empty());
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key("second-access"))
            .await
            .unwrap()
            .is_none()
    );
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_access_tokens")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0);
    storage.close().await;
}

#[tokio::test]
async fn deleting_account_oauth_removes_all_grants_but_preserves_other_access_and_history() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("delete-oauth.sqlite"))
        .await
        .unwrap();
    account(&storage, "owner").await;
    account(&storage, "other").await;
    let key = storage
        .create_proxy_api_key("owner", Some("keep-key"))
        .await
        .unwrap();
    let expires = chrono::Utc::now().timestamp() + 3600;
    for (name, owner) in [("one", "owner"), ("two", "owner"), ("other", "other")] {
        let credential = storage.create_oauth_credential(owner, name).await.unwrap();
        let rt = storage
            .oauth_refresh_token(&credential.id)
            .await
            .unwrap()
            .unwrap();
        storage
            .register_oauth_access(&credential, &rt, name, expires, &Default::default())
            .await
            .unwrap();
        storage
            .insert_usage(&codex2api_storage::UsageRecord {
                id: name.into(),
                account_id: owner.into(),
                api_key_id: credential.id,
                api_key_name: name.into(),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    assert_eq!(
        storage
            .delete_account_oauth_credentials("owner")
            .await
            .unwrap(),
        2
    );
    assert!(
        storage
            .oauth_credentials_for_account("owner")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        storage
            .oauth_devices_for_account("owner")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key("one"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key("two"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .lookup_oauth_access_hash(&hash_api_key("other"))
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        storage
            .oauth_devices_for_account("other")
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        storage
            .query_usage(&Default::default())
            .await
            .unwrap()
            .total,
        3
    );
    assert!(
        storage
            .lookup_proxy_api_key(&key.token)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        storage
            .load_account_tokens("owner")
            .await
            .unwrap()
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("official-refresh-owner")
    );
    storage.close().await;
}

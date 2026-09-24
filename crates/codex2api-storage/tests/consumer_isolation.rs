use codex2api_storage::{NewSupplierAccount, Storage, VirtualAccount};
use serde_json::json;

async fn consumer(storage: &Storage, id: &str, provider: &str, plan: &str) -> VirtualAccount {
    let account = VirtualAccount {
        provider_id: provider.into(),
        id: id.into(),
        username: id.into(),
        password_hash: "fixture".into(),
        name: id.into(),
        email: format!("{id}@example.test"),
        plan_type: "plus".into(),
        plan_id: plan.into(),
        subscription_expires_at: None,
        enabled: true,
        created_at: "2026-09-22T00:00:00Z".into(),
    };
    storage.save_virtual_account(&account).await.unwrap();
    account
}

#[tokio::test]
async fn consumer_provider_is_fixed_and_supplier_binding_cannot_cross_providers() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("separation.sqlite"))
        .await
        .unwrap();
    sqlx::query("INSERT INTO providers(id,name) VALUES('test-provider','Test adapter fixture')")
        .execute(storage.pool())
        .await
        .unwrap();
    let mut other_plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    other_plan.id = "test-provider-plan".into();
    other_plan.provider_id = "test-provider".into();
    other_plan.name = "Test".into();
    storage.save_virtual_plan(&other_plan, None).await.unwrap();
    let mut crossed = storage.virtual_plan("plus").await.unwrap().unwrap();
    let original = crossed.config.clone();
    crossed.provider_id = "test-provider".into();
    crossed.config["primary_cost_limit_usd"] = json!(123);
    assert!(
        !storage
            .save_virtual_plan(&crossed, Some(crossed.revision))
            .await
            .unwrap()
    );
    assert_eq!(
        storage.virtual_plan("plus").await.unwrap().unwrap().config,
        original
    );
    let mut a = consumer(&storage, "a", "chatgpt", "plus").await;
    let b = consumer(&storage, "b", "test-provider", &other_plan.id).await;
    a.provider_id = "test-provider".into();
    a.plan_id = other_plan.id.clone();
    assert!(storage.save_virtual_account(&a).await.is_err());
    assert_eq!(
        storage
            .virtual_account(&a.id)
            .await
            .unwrap()
            .unwrap()
            .provider_id,
        "chatgpt"
    );
    let mut new = NewSupplierAccount::pending_identity(
        "fixture-installation",
        "fixture",
        "fixture",
        "fixture",
        "fixture",
        "fixture",
        "",
        "{}",
    );
    new.provider_id = "test-provider".into();
    let supplier = storage.create_account(new).await.unwrap();
    assert!(sqlx::query("INSERT INTO execution_routes(virtual_account_id,provider_id,supplier_account_id) VALUES(?,'chatgpt',?)").bind(&a.id).bind(&supplier.id).execute(storage.pool()).await.is_err());
    assert!(sqlx::query("INSERT INTO execution_routes(virtual_account_id,provider_id,supplier_account_id) VALUES(?,'test-provider',?)").bind(&a.id).bind(&supplier.id).execute(storage.pool()).await.is_err());
    sqlx::query("INSERT INTO execution_routes(virtual_account_id,provider_id,supplier_account_id) VALUES(?,'test-provider',?)").bind(&b.id).bind(&supplier.id).execute(storage.pool()).await.unwrap();
    storage
        .save_virtual_resource(
            &b.id,
            "conversation",
            "owned",
            None,
            &json!({"title":"consumer history"}),
        )
        .await
        .unwrap();
    assert!(
        storage
            .virtual_resource(&a.id, "conversation", "owned")
            .await
            .unwrap()
            .is_none()
    );
    storage.delete_account(&supplier.id).await.unwrap();
    assert!(storage.virtual_account(&b.id).await.unwrap().is_some());
    assert!(
        storage
            .execution_route(&b.id, "test-provider")
            .await
            .unwrap()
            .unwrap()
            .supplier_account_id
            .is_none()
    );
    assert!(
        storage
            .virtual_resource(&b.id, "conversation", "owned")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn route_clear_and_supplier_deletion_keep_monotonic_revisions() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("revisions.sqlite"))
        .await
        .unwrap();
    let account = consumer(&storage, "consumer", "chatgpt", "plus").await;
    let mut supplier = NewSupplierAccount::pending_identity(
        "installation",
        "originator",
        "ua",
        "os",
        "version",
        "arch",
        "",
        "{}",
    );
    supplier.chatgpt_account_id = Some("official".into());
    let supplier = storage
        .save_authorized_account(
            supplier,
            codex2api_storage::SupplierTokens {
                access_token: Some("fixture".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        storage
            .save_execution_route(&account.id, "chatgpt", Some(&supplier.id), None)
            .await
            .unwrap()
    );
    assert!(
        storage
            .save_execution_route(&account.id, "chatgpt", None, Some(1))
            .await
            .unwrap()
    );
    assert_eq!(
        storage
            .execution_route(&account.id, "chatgpt")
            .await
            .unwrap()
            .unwrap()
            .revision,
        2
    );
    assert!(
        !storage
            .save_execution_route(&account.id, "chatgpt", Some(&supplier.id), Some(1))
            .await
            .unwrap()
    );
    assert!(
        storage
            .save_execution_route(&account.id, "chatgpt", Some(&supplier.id), Some(2))
            .await
            .unwrap()
    );
    assert!(
        !storage
            .save_execution_route(&account.id, "chatgpt", None, Some(1))
            .await
            .unwrap()
    );
    storage.delete_account(&supplier.id).await.unwrap();
    let cleared = storage
        .execution_route(&account.id, "chatgpt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cleared.revision, 4);
    assert!(cleared.supplier_account_id.is_none());
    assert!(
        storage
            .save_execution_route("missing", "chatgpt", None, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn supplier_identity_and_login_conflicts_cannot_cross_providers() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("supplier-isolation.sqlite"))
        .await
        .unwrap();
    sqlx::query("INSERT INTO providers(id,name) VALUES('test-provider','Test')")
        .execute(storage.pool())
        .await
        .unwrap();
    let mut other = NewSupplierAccount::pending_identity(
        "other-installation",
        "originator",
        "ua",
        "os",
        "version",
        "arch",
        "",
        "{}",
    );
    other.provider_id = "test-provider".into();
    other.chatgpt_account_id = Some("official".into());
    assert!(storage.create_account(other.clone()).await.is_err());
    other.chatgpt_account_id = None;
    let supplier = storage.create_account(other).await.unwrap();
    assert!(
        storage
            .update_account(
                &supplier.id,
                codex2api_storage::SupplierAccountUpdate {
                    chatgpt_account_id: Some("official".into()),
                    ..Default::default()
                }
            )
            .await
            .is_err()
    );
    // A corrupt legacy foreign identity still cannot be selected by ChatGPT login.
    sqlx::query("UPDATE supplier_accounts SET chatgpt_account_id='official' WHERE id=?")
        .bind(&supplier.id)
        .execute(storage.pool())
        .await
        .unwrap();
    let mut official = NewSupplierAccount::pending_identity(
        "official-installation",
        "originator",
        "ua",
        "os",
        "version",
        "arch",
        "",
        "{}",
    );
    official.chatgpt_account_id = Some("official".into());
    assert!(
        storage
            .save_authorized_account(
                official,
                codex2api_storage::SupplierTokens {
                    access_token: Some("new-secret".into()),
                    ..Default::default()
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        storage
            .load_supplier_tokens(&supplier.id)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .require_account(&supplier.id)
            .await
            .unwrap()
            .installation_id,
        "other-installation"
    );
}

#[tokio::test]
async fn consumer_writes_cannot_change_service_policy_or_another_consumers_state() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("permissions.sqlite"))
        .await
        .unwrap();
    let a = consumer(&storage, "a", "chatgpt", "plus").await;
    let b = consumer(&storage, "b", "chatgpt", "plus").await;
    for key in [
        "quota",
        "subscription_policy",
        "subscription_entitlements",
        "models",
        "computer_use_policy",
        "desktop_model_policy",
        "account_settings",
        "feature_bootstrap",
        "verified_access",
    ] {
        let saved = storage.virtual_config(&a.id, key).await.unwrap();
        assert!(
            storage
                .update_virtual_client_config(&a.id, key, &saved.value, saved.revision)
                .await
                .is_err(),
            "{key}"
        );
        assert!(
            storage
                .save_virtual_client_state(&a.id, key, &saved.value, None)
                .await
                .is_err(),
            "{key}"
        );
    }
    let saved = storage
        .virtual_config(&a.id, "user_settings")
        .await
        .unwrap();
    let mut escalated = saved.value.clone();
    escalated["flags"]["admin"] = json!(true);
    assert!(
        storage
            .update_virtual_client_config(&a.id, "user_settings", &escalated, saved.revision)
            .await
            .is_err()
    );
    let mut permitted = saved.value;
    permitted["settings"]["show_model_picker_slider"] = json!(true);
    storage
        .update_virtual_client_config(&a.id, "user_settings", &permitted, saved.revision)
        .await
        .unwrap();
    assert_ne!(
        storage
            .virtual_config(&b.id, "user_settings")
            .await
            .unwrap()
            .value,
        permitted
    );
}

#[tokio::test]
async fn refresh_credentials_cannot_expand_oauth_scopes_or_revoke_other_providers() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("scopes.sqlite"))
        .await
        .unwrap();
    let account = consumer(&storage, "consumer", "chatgpt", "plus").await;
    let device = storage
        .create_virtual_device(&account, "refresh", &Default::default())
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE virtual_devices SET scopes='openid profile' WHERE id=?")
        .bind(&device)
        .execute(storage.pool())
        .await
        .unwrap();
    let expires = chrono::Utc::now().timestamp() + 600;
    assert!(
        !storage
            .register_virtual_access_scoped(
                &device,
                "refresh",
                "too-wide",
                expires,
                Some("openid profile api.connectors.invoke")
            )
            .await
            .unwrap()
    );
    assert!(
        storage
            .register_virtual_access_scoped(&device, "refresh", "narrow", expires, Some("openid"))
            .await
            .unwrap()
    );
    let access = storage
        .virtual_access(&codex2api_storage::hash_token("narrow"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(access.scopes, "openid");
    assert_eq!(access.virtual_account_id, account.id);
    storage
        .revoke_virtual_token("refresh", "other-provider")
        .await
        .unwrap();
    assert!(
        storage
            .virtual_refresh_device("refresh")
            .await
            .unwrap()
            .is_some()
    );
    storage
        .revoke_virtual_token("refresh", "chatgpt")
        .await
        .unwrap();
    assert!(
        storage
            .virtual_access(&codex2api_storage::hash_token("narrow"))
            .await
            .unwrap()
            .is_none()
    );
}

#[cfg(test)]
mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
#[cfg(test)]
use account_fixture::AccountFixture;
use chrono::{Duration, Utc};
use codex2api_storage::{PlatformAccount, Storage, SubscriptionChange, User, hash_token};
use serde_json::json;

async fn setup() -> (tempfile::TempDir, Storage, User) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("users.sqlite"))
        .await
        .unwrap();
    let user = User {
        kind: codex2api_storage::UserKind::Regular,
        id: "user-a".into(),
        username: "alice".into(),
        password_hash: codex2api_storage::hash_password("password").unwrap(),
        name: "Alice".into(),
        email: "alice@user.test".into(),
        enabled: true,
        wallet_cents: 99999,
        revision: 1,
        created_at: Utc::now().to_rfc3339(),
    };
    assert!(storage.save_user(&user, None).await.unwrap());
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        0
    );
    storage
        .save_supplier_tag("pool", "chatgpt", "Private pool")
        .await
        .unwrap();
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["sale_price_usd"] = json!("10.25");
    plan.config["duration_days"] = json!(30);
    plan.config["supplier_tag_id"] = json!("pool");
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    (dir, storage, user)
}
#[tokio::test]
async fn administrator_wallet_adjustments_are_atomic_idempotent_and_auditable() {
    use codex2api_storage::WalletAdjustment;
    let (_dir, storage, user) = setup().await;
    let admin = storage.require_admin_user().await.unwrap();
    let input = |key: &str, amount, revision| WalletAdjustment {
        request_id: key.into(),
        amount_cents: amount,
        expected_revision: revision,
        reason: Some("Audit fixture".into()),
    };
    let first = storage
        .adjust_user_wallet(&user.id, admin.id, input("increase", 2500, 1))
        .await
        .unwrap();
    assert_eq!(first.kind, "system_adjustment");
    assert_eq!(first.balance_before_cents, 0);
    assert_eq!(first.balance_cents, 2500);
    assert_eq!(
        first.operator_account_id.as_deref(),
        Some(admin.account_id.as_str())
    );
    assert_eq!(
        storage
            .adjust_user_wallet(&user.id, admin.id, input("increase", 2500, 1))
            .await
            .unwrap()
            .id,
        first.id
    );
    assert!(
        storage
            .adjust_user_wallet(&user.id, admin.id, input("increase", 5000, 1))
            .await
            .is_err()
    );
    assert!(
        storage
            .adjust_user_wallet(&user.id, admin.id, input("excess", -2501, 2))
            .await
            .is_err()
    );
    assert!(
        storage
            .adjust_user_wallet(&user.id, admin.id, input("stale", -500, 1))
            .await
            .is_err()
    );
    let (a, b) = tokio::join!(
        storage.adjust_user_wallet(&user.id, admin.id, input("decrease-a", -500, 2)),
        storage.adjust_user_wallet(&user.id, admin.id, input("decrease-b", -500, 2))
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        2000
    );
    let entries = storage.wallet_entries(&user.id).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries.iter().map(|e| e.amount_cents).sum::<i64>(), 2000);
    let visible = storage.user_store().wallet_entries(&user.id).await.unwrap();
    for entry in visible {
        assert!(entry.get("operator_name").is_none());
        assert!(entry.get("reason").is_none());
        assert!(entry.get("operator_account_id").is_none());
    }
    assert!(
        sqlx::query("UPDATE wallet_entries SET amount_cents=1 WHERE id=?")
            .bind(&first.id)
            .execute(storage.pool())
            .await
            .is_err()
    );
    storage.close().await;
}

#[tokio::test]
async fn wallet_adjustment_reason_can_be_missing_null_or_blank() {
    let (_dir, storage, user) = setup().await;
    let admin = storage.require_admin_user().await.unwrap();
    for (index, reason) in [
        None,
        Some(serde_json::Value::Null),
        Some(json!("")),
        Some(json!(" \t ")),
    ]
    .into_iter()
    .enumerate()
    {
        let mut body = json!({"request_id":format!("optional-reason-{index}"),"amount_cents":50,"expected_revision":index as i64+1});
        if let Some(reason) = reason {
            body["reason"] = reason;
        }
        let input = serde_json::from_value(body.clone()).unwrap();
        let entry = storage
            .adjust_user_wallet(&user.id, admin.id, input)
            .await
            .unwrap();
        assert!(entry.reason.is_none());
        assert!(entry.operator_account_id.is_some());
        assert_eq!(entry.amount_cents, 50);
        let replay = storage
            .adjust_user_wallet(&user.id, admin.id, serde_json::from_value(body).unwrap())
            .await
            .unwrap();
        assert_eq!(replay.id, entry.id);
    }
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        200
    );
    assert_eq!(storage.wallet_entries(&user.id).await.unwrap().len(), 4);
    storage.close().await;
}
#[tokio::test]
async fn virtual_users_share_identity_storage_but_not_user_business() {
    let (_dir, storage, user) = setup().await;
    let account = PlatformAccount {
        id: "standalone".into(),
        provider_id: "chatgpt".into(),
        username: "virtual-alice".into(),
        password_hash: user.password_hash.clone(),
        name: "Independent".into(),
        email: "independent@virtual.test".into(),
        plan_id: "plus".into(),
        plan_type: "plus".into(),
        subscription_expires_at: None,
        enabled: true,
        created_at: Utc::now().to_rfc3339(),
    };
    storage.save_account_fixture(&account).await.unwrap();
    assert_eq!(storage.users().await.unwrap().len(), 1);
    assert_eq!(
        storage.user_subscriptions(None, true).await.unwrap()[0].plan_type,
        "free"
    );
    assert_eq!(
        storage
            .user_platform_account(&user.id, "chatgpt")
            .await
            .unwrap()
            .unwrap()
            .plan_type,
        "free"
    );
    let expires = (Utc::now() + Duration::days(30)).to_rfc3339();
    storage
        .save_user_subscription(SubscriptionChange {
            reissue: false,
            user_id: &user.id,
            plan_id: "plus",
            expires_at: Some(&expires),
            enabled: true,
            revision: Some(1),
        })
        .await
        .unwrap();
    let platform = storage
        .user_platform_account(&user.id, "chatgpt")
        .await
        .unwrap()
        .unwrap();
    assert_ne!(platform.id, account.id);
    assert_eq!(storage.platform_accounts().await.unwrap().len(), 1);
    let internal = storage
        .platform_account(&platform.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(internal.username, user.username);
    let virtual_user = storage
        .user_by_username("virtual-alice")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(virtual_user.kind, codex2api_storage::UserKind::Virtual);
    assert!(
        storage
            .create_user_session(&virtual_user, "csrf")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .user_store()
            .user(&virtual_user.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(storage.save_account_fixture(&platform).await.is_err());
    assert!(storage.delete_virtual_user(&platform.id).await.is_err());
    assert!(
        storage
            .save_user_subscription(SubscriptionChange {
                reissue: false,
                user_id: &user.id,
                plan_id: "plus",
                expires_at: Some(&expires),
                enabled: true,
                revision: None
            })
            .await
            .is_err()
    );
    storage.close().await;
}
#[tokio::test]
async fn expiration_and_credential_changes_apply_to_all_user_devices_but_not_standalone_accounts() {
    let (_dir, storage, mut user) = setup().await;
    let expiry = (Utc::now() + Duration::days(1)).to_rfc3339();
    storage
        .save_user_subscription(SubscriptionChange {
            reissue: false,
            user_id: &user.id,
            plan_id: "plus",
            expires_at: Some(&expiry),
            enabled: true,
            revision: Some(1),
        })
        .await
        .unwrap();
    let account = storage
        .user_platform_account(&user.id, "chatgpt")
        .await
        .unwrap()
        .unwrap();
    let device = storage
        .create_virtual_device(&account, "refresh", &Default::default())
        .await
        .unwrap()
        .unwrap();
    storage
        .register_virtual_access(&device, "refresh", "access", Utc::now().timestamp() + 600)
        .await
        .unwrap();
    let browser = storage
        .create_user_session(&user, "csrf")
        .await
        .unwrap()
        .unwrap();
    let sub = storage
        .user_subscriptions(None, true)
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.provider_id == "chatgpt")
        .unwrap();
    let expired = (Utc::now() - Duration::hours(1)).to_rfc3339();
    storage
        .save_user_subscription(SubscriptionChange {
            reissue: false,
            user_id: &user.id,
            plan_id: "plus",
            expires_at: Some(&expired),
            enabled: true,
            revision: Some(sub.revision),
        })
        .await
        .unwrap();
    assert!(
        storage
            .user_subscriptions(None, false)
            .await
            .unwrap()
            .iter()
            .all(|s| s.provider_id != "chatgpt")
    );
    assert_eq!(
        storage
            .platform_account(&account.id)
            .await
            .unwrap()
            .unwrap()
            .effective_plan(),
        "free"
    );
    assert!(storage.user_session(&browser).await.unwrap().is_some());
    assert!(
        storage
            .virtual_access(&hash_token("access"))
            .await
            .unwrap()
            .is_some()
    );
    user.enabled = false;
    assert!(storage.save_user(&user, Some(user.revision)).await.unwrap());
    assert!(storage.user_session(&browser).await.unwrap().is_none());
    assert!(
        storage
            .virtual_refresh_device("refresh")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .virtual_access(&hash_token("access"))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .user_platform_account(&user.id, "chatgpt")
            .await
            .unwrap()
            .is_none()
    );
    storage.close().await;
}

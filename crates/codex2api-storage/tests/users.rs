use chrono::{Duration, Utc};
use codex2api_storage::{Storage, SubscriptionChange, User, hash_token};
use serde_json::json;

async fn setup() -> (
    codex2api_storage::test_support::TestDirectory,
    Storage,
    User,
) {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("users-pg"))
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
    (dir, storage, user)
}

#[tokio::test]
async fn a_locked_wallet_does_not_block_another_users_transaction() {
    let (_dir, storage, user) = setup().await;
    let other = User {
        id: "other-owner".into(),
        username: "other-owner".into(),
        ..user.clone()
    };
    storage.save_user(&other, None).await.unwrap();
    let mut locked = storage.pool().begin().await.unwrap();
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(&user.id)
        .execute(&mut *locked)
        .await
        .unwrap();
    let admin = storage.require_admin_user().await.unwrap();
    let entry = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        storage.adjust_user_wallet(
            &other.id,
            admin.id,
            codex2api_storage::WalletAdjustment {
                request_id: "independent-owner".into(),
                amount_cents: 25,
                expected_revision: 1,
                reason: None,
            },
        ),
    )
    .await
    .expect("independent users must not share a write lock")
    .unwrap();
    assert_eq!(entry.balance_cents, 25);
    locked.rollback().await.unwrap();
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        0
    );
}

#[tokio::test]
async fn login_attempt_limits_are_atomic_case_insensitive_and_survive_reconnect() {
    let (_dir, storage, _) = setup().await;
    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..20 {
        let storage = storage.clone();
        tasks.spawn(async move {
            storage
                .allow_user_login_attempt(if index % 2 == 0 { "Alice" } else { "alice" })
                .await
                .unwrap()
        });
    }
    let mut allowed = 0;
    while let Some(result) = tasks.join_next().await {
        allowed += usize::from(result.unwrap());
    }
    assert_eq!(allowed, 10);
    let reopened = storage.reopen().await.unwrap();
    assert!(!reopened.allow_user_login_attempt("ALICE").await.unwrap());
    assert!(reopened.allow_user_login_attempt("bob").await.unwrap());
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
            .adjust_user_wallet(&user.id, admin.id, input("excess", -2501, 1))
            .await
            .is_err()
    );
    let (a, b) = tokio::join!(
        storage.adjust_user_wallet(&user.id, admin.id, input("decrease", -500, 1)),
        storage.adjust_user_wallet(&user.id, admin.id, input("increase-b", 250, 1))
    );
    assert_eq!(a.unwrap().amount_cents, -500);
    assert_eq!(b.unwrap().amount_cents, 250);
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        2250
    );
    // Both withdrawals use the same identity revision, but only the first can
    // spend the current funds. The rejected transaction must not append a row.
    let (a, b) = tokio::join!(
        storage.adjust_user_wallet(&user.id, admin.id, input("withdraw-a", -1500, 1)),
        storage.adjust_user_wallet(&user.id, admin.id, input("withdraw-b", -1500, 1))
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let mut current = storage.user(&user.id).await.unwrap().unwrap();
    assert_eq!(current.wallet_cents, 750);
    assert_eq!(current.revision, user.revision);
    let entries = storage.wallet_entries(&user.id).await.unwrap();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries.iter().map(|e| e.amount_cents).sum::<i64>(), 750);
    current.name = "Updated identity".into();
    assert!(
        storage
            .save_user(&current, Some(current.revision))
            .await
            .unwrap()
    );
    assert!(matches!(
        storage
            .adjust_user_wallet(&user.id, admin.id, input("stale-identity", 100, 1))
            .await,
        Err(codex2api_storage::StorageError::ProxyChanged)
    ));
    assert_eq!(
        storage.user(&user.id).await.unwrap().unwrap().wallet_cents,
        750
    );
    assert_eq!(storage.wallet_entries(&user.id).await.unwrap().len(), 4);
    let visible = storage
        .user_store()
        .wallet_entries(&user.id, &Default::default())
        .await
        .unwrap()
        .items;
    for entry in visible {
        assert!(entry.get("operator_name").is_none());
        assert!(entry.get("reason").is_none());
        assert!(entry.get("operator_account_id").is_none());
    }
    assert!(
        sqlx::query("UPDATE wallet_entries SET amount_cents=1 WHERE id=$1")
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
        let mut body = json!({"request_id":format!("optional-reason-{index}"),"amount_cents":50,"expected_revision":user.revision});
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
async fn expiration_blocks_execution_and_credential_changes_revoke_user_devices() {
    let (_dir, storage, mut user) = setup().await;
    let expiry = (Utc::now() + Duration::days(1)).to_rfc3339();
    let subscription_id = storage
        .save_user_subscription(SubscriptionChange {
            subscription_id: None,
            reissue: false,
            user_id: &user.id,
            plan_id: "plus",
            expires_at: Some(&expiry),
            enabled: true,
            revision: None,
        })
        .await
        .unwrap();
    let account = storage
        .platform_account(&subscription_id)
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
            subscription_id: Some(&sub.virtual_account_id),
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
            .plan_type
            .as_str(),
        "plus"
    );
    assert!(
        !storage
            .effective_entitlements(&account.id)
            .await
            .unwrap()
            .execution_enabled
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
        !storage
            .platform_account(&subscription_id)
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
    storage.close().await;
}

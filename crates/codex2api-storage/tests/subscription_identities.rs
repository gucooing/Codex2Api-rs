use codex2api_storage::{Storage, SubscriptionChange, User, UserKind};
use serde_json::json;

async fn user(storage: &Storage) -> User {
    let user = User {
        id: "owner".into(),
        kind: UserKind::Regular,
        username: "owner".into(),
        password_hash: "fixture".into(),
        name: "Owner".into(),
        email: "owner@example.test".into(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_user(&user, None).await.unwrap();
    user
}
async fn grant(storage: &Storage, owner: &str) -> String {
    storage
        .save_user_subscription(SubscriptionChange {
            subscription_id: None,
            reissue: false,
            user_id: owner,
            plan_id: "plus",
            expires_at: None,
            enabled: true,
            revision: None,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn subscriptions_are_explicit_and_same_platform_limits_and_state_are_independent() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("subscriptions-pg"))
        .await
        .unwrap();
    let owner = user(&storage).await;
    assert!(
        storage
            .user_subscriptions(Some(&owner.id), true)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        storage
            .virtual_plans()
            .await
            .unwrap()
            .iter()
            .all(|plan| plan.plan_type != "free")
    );
    let first = grant(&storage, &owner.id).await;
    let second = grant(&storage, &owner.id).await;
    assert_ne!(first, second);
    let defaults = storage.virtual_rpm_limit(&first).await.unwrap();
    assert_eq!(
        (defaults.effective_rpm, defaults.effective_concurrency),
        (40, 10)
    );
    let visible = storage
        .user_store()
        .subscriptions(&owner.id, &Default::default())
        .await
        .unwrap();
    assert_eq!(visible.total, 2);
    assert!(
        visible
            .items
            .iter()
            .all(|item| item["rpm_limit"] == 40 && item["concurrency_limit"] == 10)
    );
    assert!(
        !serde_json::to_string(&visible)
            .unwrap()
            .contains("supplier_tag_id")
    );
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["rpm_limit"] = 2.into();
    plan.config["concurrency_limit"] = 1.into();
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    for id in [&first, &second] {
        assert!(
            storage
                .admit_virtual_request(id, true)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            storage
                .admit_virtual_request(id, true)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            storage.admit_virtual_request(id, true).await.unwrap(),
            Some(60)
        );
    }
    let permit = storage.acquire_consumer(&first).await.unwrap().unwrap();
    assert!(storage.acquire_consumer(&first).await.unwrap().is_none());
    let other = storage.acquire_consumer(&second).await.unwrap().unwrap();
    drop((permit, other));
    storage
        .save_virtual_request_limits(&second, Some(0), Some(3))
        .await
        .unwrap();
    assert!(
        storage
            .admit_virtual_request(&second, true)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .virtual_rpm_limit(&first)
            .await
            .unwrap()
            .effective_concurrency,
        1
    );
    assert_eq!(
        storage
            .virtual_rpm_limit(&second)
            .await
            .unwrap()
            .effective_concurrency,
        3
    );
    storage
        .save_virtual_client_state(
            &first,
            "cloud_preferences",
            &json!({"branch_format":"first/{task_id}"}),
            None,
        )
        .await
        .unwrap();
    assert!(
        storage
            .virtual_client_state(&second, "cloud_preferences")
            .await
            .unwrap()
            .is_none()
    );
    let revision = storage.platform_revision(&first).await.unwrap();
    storage
        .save_user_subscription(SubscriptionChange {
            subscription_id: Some(&first),
            reissue: false,
            user_id: &owner.id,
            plan_id: "plus",
            expires_at: Some("2020-01-01T00:00:00Z"),
            enabled: true,
            revision: Some(revision),
        })
        .await
        .unwrap();
    assert!(
        !storage
            .effective_entitlements(&first)
            .await
            .unwrap()
            .execution_enabled
    );
    assert!(
        storage
            .effective_entitlements(&second)
            .await
            .unwrap()
            .execution_enabled
    );
    assert_eq!(
        storage
            .platform_account(&first)
            .await
            .unwrap()
            .unwrap()
            .plan_id,
        "plus"
    );
    assert_eq!(
        storage
            .oauth_subscriptions(&owner.id, "chatgpt")
            .await
            .unwrap()
            .len(),
        1
    );
    storage.delete_user_subscription(&first).await.unwrap();
    assert!(storage.user(&owner.id).await.unwrap().is_some());
    assert!(storage.platform_account(&second).await.unwrap().is_some());
    storage.close().await;
}

#[cfg(test)]
mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
#[cfg(test)]
use account_fixture::AccountFixture;
use codex2api_storage::{
    NewSupplierAccount, PlatformAccount, Storage, SupplierStatus, SupplierTokens,
};

async fn supplier(storage: &Storage, id: &str) {
    supplier_for(storage, id, "chatgpt").await;
}

#[tokio::test]
async fn request_journal_commits_valid_rows_and_settles_each_request_once() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("journal-pg"))
        .await
        .unwrap();
    supplier(&storage, "supplier").await;
    consumer(&storage, "owner").await;
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["spending_windows"] = serde_json::json!([
        {"duration_seconds":2592000,"cost_limit_usd":"1000"},
        {"duration_seconds":18000,"cost_limit_usd":"0.000000001"}
    ]);
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    assert_eq!(
        storage.virtual_spending_status("owner").await.unwrap(),
        (true, true)
    );
    assert_eq!(
        storage.execution_quota("owner").await.unwrap()["rate_limit"]["primary_window"]["limit_window_seconds"],
        2592000
    );
    storage
        .sync_supported_models(&[codex2api_common::SupportedModel {
            provider_id: "chatgpt".into(),
            model: "gpt-5.5".into(),
            kind: "text".into(),
            preset_key: None,
        }])
        .await
        .unwrap();
    let record = codex2api_storage::UsageRecord {
        id: "request".into(),
        account_id: "supplier".into(),
        subject_id: "owner".into(),
        subject_kind: "virtual_account".into(),
        provider_id: "chatgpt".into(),
        status: "in_progress".into(),
        endpoint: "/v1/responses".into(),
        transport: "http".into(),
        model: Some("gpt-5.5".into()),
        requested_at_ms: chrono::Utc::now().timestamp_millis(),
        ..Default::default()
    };
    let (first, duplicate) =
        tokio::join!(storage.insert_usage(&record), storage.insert_usage(&record));
    assert_ne!(
        first.is_ok(),
        duplicate.is_ok(),
        "first={first:?}, duplicate={duplicate:?}"
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM usage_records WHERE id='request'")
        .fetch_one(storage.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        storage.execution_quota("owner").await.unwrap()["rate_limit"]["primary_window"]["started_at"],
        record.requested_at_ms / 1000
    );
    let mut completed = record;
    completed.status = "completed".into();
    completed.input_tokens = Some(10);
    completed.output_tokens = Some(3);
    storage.finish_usage(&completed).await.unwrap();
    assert_eq!(
        storage.virtual_spending_status("owner").await.unwrap(),
        (false, true)
    );
    completed.input_tokens = Some(999);
    storage.finish_usage(&completed).await.unwrap();
    let row: (String, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT status,input_tokens,cost_nano_usd FROM usage_records WHERE id='request'",
    )
    .fetch_one(storage.pool())
    .await
    .unwrap();
    assert_eq!(row.0, "completed");
    assert_eq!(row.1, Some(10));
    assert!(row.2.is_some_and(|cost| cost > 0));
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.config["spending_windows"][1]["cost_limit_usd"] = "10".into();
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    assert_eq!(
        storage.virtual_spending_status("owner").await.unwrap(),
        (true, true)
    );
}

#[tokio::test]
async fn shared_snapshots_refresh_models_transport_and_availability_for_each_provider() {
    use std::sync::Arc;
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("snapshots-pg"))
        .await
        .unwrap();
    for provider in ["chatgpt", "grok"] {
        supplier_for(&storage, provider, provider).await;
        storage
            .save_supplier_tag(provider, provider, provider)
            .await
            .unwrap();
        storage
            .edit_supplier_tags(&[provider.into()], &[provider.into()], false)
            .await
            .unwrap();
        storage
            .sync_supported_models(&[codex2api_common::SupportedModel {
                provider_id: provider.into(),
                model: "shared-model".into(),
                kind: "text".into(),
                preset_key: None,
            }])
            .await
            .unwrap();
        let empty = storage
            .pool_candidates(provider, provider, Some("shared-model"))
            .await
            .unwrap();
        assert!(empty.is_empty());
        storage
            .add_supplier_model(provider, "shared-model")
            .await
            .unwrap();
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..3 {
            let storage = storage.clone();
            tasks.spawn(async move {
                storage
                    .pool_candidates(provider, provider, Some("shared-model"))
                    .await
                    .unwrap()
            });
        }
        let first = tasks.join_next().await.unwrap().unwrap();
        assert_eq!(first.len(), 1);
        while let Some(result) = tasks.join_next().await {
            assert!(Arc::ptr_eq(&first, &result.unwrap()));
        }
        assert!(empty.is_empty());
        let models = storage.supplier_model_snapshot(provider).await.unwrap();
        assert!(models.contains("shared-model"));
        let catalog = storage.model_catalog_snapshot(provider).await.unwrap();
        assert!(catalog.iter().all(|m| m.provider_id == provider));
        storage
            .remove_supplier_model(provider, "shared-model")
            .await
            .unwrap();
        assert!(
            !storage
                .supplier_supports_model(provider, "shared-model")
                .await
                .unwrap()
        );
        assert!(models.contains("shared-model"));
        assert!(
            storage
                .pool_candidates(provider, provider, Some("shared-model"))
                .await
                .unwrap()
                .is_empty()
        );

        let initial = storage.supplier_transport_snapshot(provider).await.unwrap();
        assert!(Arc::ptr_eq(
            &initial,
            &storage.supplier_transport_snapshot(provider).await.unwrap()
        ));
        let proxy = storage
            .create_outbound_proxy("proxy", "http://127.0.0.1:12345")
            .await
            .unwrap();
        storage
            .set_account_proxy(provider, Some(&proxy.id))
            .await
            .unwrap();
        let bound = storage.supplier_transport_snapshot(provider).await.unwrap();
        assert_eq!(bound.proxy_url.as_deref(), Some("http://127.0.0.1:12345/"));
        storage
            .update_outbound_proxy(&proxy.id, "changed", "http://127.0.0.1:12346")
            .await
            .unwrap();
        assert_eq!(
            storage
                .supplier_transport_snapshot(provider)
                .await
                .unwrap()
                .proxy_url
                .as_deref(),
            Some("http://127.0.0.1:12346/")
        );
        let mut account = storage.require_account(provider).await.unwrap();
        account.os_version = "1.2.3".into();
        storage.save_account_fingerprint(&account).await.unwrap();
        assert_eq!(
            storage
                .supplier_transport_snapshot(provider)
                .await
                .unwrap()
                .os_version,
            "1.2.3"
        );
        assert_ne!(initial.os_version, "1.2.3");
        storage
            .delete_outbound_proxy(&proxy.id, true)
            .await
            .unwrap();
        assert!(
            storage
                .supplier_transport_snapshot(provider)
                .await
                .unwrap()
                .proxy_url
                .is_none()
        );

        let revision = storage
            .supplier_auth_revision(provider)
            .await
            .unwrap()
            .unwrap();
        let reset = chrono::Utc::now().timestamp() + 1;
        storage
            .exhaust_supplier_quota(provider, revision, reset, "fixture")
            .await
            .unwrap();
        assert!(
            storage
                .pool_candidates(provider, provider, None)
                .await
                .unwrap()
                .is_empty()
        );
        let millis = (reset * 1000 - chrono::Utc::now().timestamp_millis()).max(0) as u64 + 25;
        tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
        assert_eq!(
            storage
                .pool_candidates(provider, provider, None)
                .await
                .unwrap()
                .len(),
            1
        );
        storage
            .set_account_status(provider, SupplierStatus::Disabled)
            .await
            .unwrap();
        assert!(
            storage
                .pool_candidates(provider, provider, None)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
async fn supplier_for(storage: &Storage, id: &str, provider: &str) {
    let mut new = NewSupplierAccount::pending_identity(id, "Linux", "6", "x86_64", "{}");
    new.id = Some(id.into());
    new.provider_id = provider.into();
    new.chatgpt_account_id = Some(id.into());
    new.chatgpt_user_id = Some(format!("user-{id}"));
    storage
        .save_authorized_account(
            new,
            SupplierTokens {
                access_token: Some("fixture".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
}
async fn consumer(storage: &Storage, id: &str) {
    storage
        .save_account_fixture(&PlatformAccount {
            id: id.into(),
            provider_id: "chatgpt".into(),
            username: id.into(),
            password_hash: "fixture".into(),
            name: id.into(),
            email: format!("{id}@example.test"),
            plan_type: "plus".into(),
            plan_id: "plus".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn granted_subscriptions_follow_plan_pools_with_provider_isolation() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("granted-pg"))
        .await
        .unwrap();
    let user = codex2api_storage::User {
        id: "user".into(),
        username: "user".into(),
        password_hash: "fixture".into(),
        name: "User".into(),
        email: "user@example.test".into(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
        kind: codex2api_storage::UserKind::Regular,
    };
    storage.save_user(&user, None).await.unwrap();
    for (provider, tier) in [("chatgpt", "plus"), ("grok", "supergrok")] {
        for suffix in ["first", "second"] {
            let id = format!("{provider}-{suffix}");
            supplier_for(&storage, &id, provider).await;
            storage.save_supplier_tag(&id, provider, &id).await.unwrap();
            storage
                .replace_supplier_tags(std::slice::from_ref(&id), std::slice::from_ref(&id))
                .await
                .unwrap();
        }
        let mut plan = storage.plan_fixture(provider).await.unwrap();
        plan.id = format!("paid-{provider}");
        plan.plan_type = tier.into();
        plan.config["supplier_tag_id"] = serde_json::json!(format!("{provider}-first"));
        storage.save_virtual_plan(&plan, None).await.unwrap();
        let subscription = storage
            .grant_subscription_fixture(&user.id, &plan.id)
            .await
            .unwrap();
        assert_eq!(
            storage
                .select_pool_supplier(&subscription.id, provider, &[])
                .await
                .unwrap(),
            Some(format!("{provider}-first"))
        );
        let mut plan = storage.virtual_plan(&plan.id).await.unwrap().unwrap();
        plan.config["supplier_tag_id"] = serde_json::json!(format!("{provider}-second"));
        storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap();
        assert_eq!(
            storage
                .select_pool_supplier(&subscription.id, provider, &[])
                .await
                .unwrap(),
            Some(format!("{provider}-second"))
        );
        if provider == "grok" {
            let mut plan = storage.virtual_plan(&plan.id).await.unwrap().unwrap();
            plan.config["supplier_tag_id"] = serde_json::json!("chatgpt-first");
            assert!(
                storage
                    .save_virtual_plan(&plan, Some(plan.revision))
                    .await
                    .is_err()
            );
            assert_eq!(
                storage
                    .select_pool_supplier(&subscription.id, provider, &[])
                    .await
                    .unwrap()
                    .as_deref(),
                Some("grok-second")
            );
        }
    }
    storage.close().await;
}

#[tokio::test]
async fn billing_rejection_survives_refresh_and_quota_recovery_but_not_reauthorization() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("billing-pg"))
        .await
        .unwrap();
    for id in ["s", "other"] {
        supplier(&storage, id).await;
    }
    consumer(&storage, "v").await;
    storage
        .save_supplier_tag("pool", "chatgpt", "Pool")
        .await
        .unwrap();
    storage
        .replace_supplier_tags(&["s".into(), "other".into()], &["pool".into()])
        .await
        .unwrap();
    storage
        .save_pool_route("v", "chatgpt", Some("pool"), Some(1))
        .await
        .unwrap();
    let auth = storage.supplier_auth_revision("s").await.unwrap().unwrap();
    let tokens = storage.load_supplier_tokens("s").await.unwrap().unwrap();
    storage
        .exhaust_supplier_quota(
            "s",
            auth,
            chrono::Utc::now().timestamp() + 600,
            "usage_limit_reached",
        )
        .await
        .unwrap();
    storage
        .mark_supplier_payment_required("s", auth, Some("deactivated_workspace"), None)
        .await
        .unwrap();
    let health = storage.supplier_health("s").await.unwrap();
    assert!(health.payment_required);
    assert!(!health.authentication_invalid);
    assert_eq!(health.display_status(true, true), "payment_required");
    assert_eq!(
        storage
            .load_supplier_tokens("s")
            .await
            .unwrap()
            .unwrap()
            .access_token,
        tokens.access_token
    );
    assert!(
        storage
            .supplier_quota_probe_candidates("chatgpt")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        storage
            .select_pool_supplier("v", "chatgpt", &[])
            .await
            .unwrap()
            .as_deref(),
        Some("other")
    );
    storage
        .recover_supplier_quota("s", auth, health.cooldown_revision)
        .await
        .unwrap();
    assert!(storage.supplier_health("s").await.unwrap().payment_required);
    let mut refreshed = tokens.clone();
    refreshed.access_token = Some("refreshed".into());
    assert!(
        storage
            .replace_supplier_tokens("s", auth, refreshed.clone())
            .await
            .unwrap()
    );
    assert!(storage.supplier_health("s").await.unwrap().payment_required);
    let current = storage.supplier_auth_revision("s").await.unwrap().unwrap();
    storage.reject_supplier_auth("s", current).await.unwrap();
    storage
        .set_account_status("s", SupplierStatus::Disabled)
        .await
        .unwrap();
    // A verified login may return identical tokens; it still clears prior failure state.
    storage.upsert_supplier_tokens(refreshed).await.unwrap();
    assert!(storage.supplier_auth_revision("s").await.unwrap().unwrap() > current);
    storage.reject_supplier_auth("s", current).await.unwrap();
    storage
        .mark_supplier_payment_required("s", current, None, None)
        .await
        .unwrap();
    let health = storage.supplier_health("s").await.unwrap();
    assert!(!health.payment_required);
    assert!(!health.authentication_invalid);
    assert_eq!(
        storage.require_account("s").await.unwrap().status,
        SupplierStatus::Disabled
    );
    storage
        .set_account_status("s", SupplierStatus::Active)
        .await
        .unwrap();
    assert_eq!(health.display_status(true, true), "active");
    storage
        .set_account_status("s", SupplierStatus::Disabled)
        .await
        .unwrap();
    let auth = storage.supplier_auth_revision("s").await.unwrap().unwrap();
    storage
        .mark_supplier_payment_required("s", auth, None, None)
        .await
        .unwrap();
    // The login path which finds an existing supplier must preserve the same intent.
    supplier(&storage, "s").await;
    assert_eq!(
        storage.require_account("s").await.unwrap().status,
        SupplierStatus::Disabled
    );
    assert!(!storage.supplier_health("s").await.unwrap().payment_required);
}

#[tokio::test]
async fn billing_reset_preserves_credentials_and_rejects_stale_probe_observations() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("billing-reset-pg"))
        .await
        .unwrap();
    supplier(&storage, "s").await;
    let auth = storage.supplier_auth_revision("s").await.unwrap().unwrap();
    storage
        .mark_supplier_payment_required("s", auth, None, None)
        .await
        .unwrap();
    let observation = storage
        .supplier_health("s")
        .await
        .unwrap()
        .cooldown_revision;
    storage.reset_supplier_availability("s").await.unwrap();
    storage
        .mark_supplier_payment_required("s", auth, None, Some(observation))
        .await
        .unwrap();
    assert!(!storage.supplier_health("s").await.unwrap().payment_required);
    assert_eq!(
        storage.supplier_auth_revision("s").await.unwrap(),
        Some(auth)
    );
    assert_eq!(
        storage
            .load_supplier_tokens("s")
            .await
            .unwrap()
            .unwrap()
            .access_token
            .as_deref(),
        Some("fixture")
    );
    storage
        .mark_supplier_payment_required("s", auth, None, None)
        .await
        .unwrap();
    assert!(storage.supplier_health("s").await.unwrap().payment_required);
}

#[tokio::test]
async fn replacing_tags_is_atomic_scoped_and_preserves_other_account_fields() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("replace-tags-pg"))
        .await
        .unwrap();
    for id in ["s1", "s2", "untouched"] {
        supplier(&storage, id).await;
    }
    for id in ["a", "b", "c"] {
        storage.save_supplier_tag(id, "chatgpt", id).await.unwrap();
    }
    storage
        .edit_supplier_tags(&["s1".into(), "untouched".into()], &["a".into()], false)
        .await
        .unwrap();
    storage
        .edit_supplier_tags(&["s1".into(), "s2".into()], &["b".into()], false)
        .await
        .unwrap();
    consumer(&storage, "v").await;
    storage
        .save_pool_route("v", "chatgpt", Some("a"), Some(1))
        .await
        .unwrap();
    let original = storage.require_account("s1").await.unwrap();
    storage
        .replace_supplier_tags(&["s1".into(), "s2".into()], &["c".into()])
        .await
        .unwrap();
    assert_eq!(storage.supplier_tag_ids("s1").await.unwrap(), vec!["c"]);
    assert_eq!(storage.supplier_tag_ids("s2").await.unwrap(), vec!["c"]);
    assert_eq!(
        storage.supplier_tag_ids("untouched").await.unwrap(),
        vec!["a"]
    );
    let updated = storage.require_account("s1").await.unwrap();
    assert_eq!(updated.installation_id, original.installation_id);
    assert_eq!(updated.status, original.status);
    assert_eq!(updated.updated_at, original.updated_at);
    assert_eq!(
        storage
            .select_pool_supplier("v", "chatgpt", &[])
            .await
            .unwrap()
            .as_deref(),
        Some("untouched")
    );
    sqlx::query("INSERT INTO providers VALUES('other','Other')")
        .execute(storage.pool())
        .await
        .unwrap();
    storage
        .save_supplier_tag("foreign", "other", "Foreign")
        .await
        .unwrap();
    for invalid in [vec!["foreign".into()], vec!["missing".into()]] {
        assert!(
            storage
                .replace_supplier_tags(&["s1".into(), "s2".into()], &invalid)
                .await
                .is_err()
        );
        assert_eq!(storage.supplier_tag_ids("s1").await.unwrap(), vec!["c"]);
        assert_eq!(storage.supplier_tag_ids("s2").await.unwrap(), vec!["c"]);
    }
    assert!(
        storage
            .replace_supplier_tags(&["s1".into(), "missing-account".into()], &[])
            .await
            .is_err()
    );
    assert_eq!(storage.supplier_tag_ids("s1").await.unwrap(), vec!["c"]);
    storage
        .replace_supplier_tags(&["s1".into()], &[])
        .await
        .unwrap();
    assert!(storage.supplier_tag_ids("s1").await.unwrap().is_empty());
    assert_eq!(storage.supplier_tag_ids("s2").await.unwrap(), vec!["c"]);
}

#[tokio::test]
async fn concurrent_pool_configuration_is_provider_isolated() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("pools-pg"))
        .await
        .unwrap();
    for id in ["s1", "s2", "outside"] {
        supplier(&storage, id).await;
    }
    storage
        .save_supplier_tag("pool", "chatgpt", "Main")
        .await
        .unwrap();
    storage
        .edit_supplier_tags(&["s1".into(), "s2".into()], &["pool".into()], false)
        .await
        .unwrap();
    let mut handles = Vec::new();
    for i in 0..20 {
        let owner = format!("v{i}");
        consumer(&storage, &owner).await;
        let s = storage.clone();
        handles.push(tokio::spawn(async move {
            s.save_pool_route(&owner, "chatgpt", Some("pool"), Some(1))
                .await
                .unwrap()
        }));
    }
    for handle in handles {
        assert!(handle.await.unwrap());
    }
    let route = storage
        .execution_route("v0", "chatgpt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        storage
            .select_pool_supplier("v0", "chatgpt", &[])
            .await
            .unwrap(),
        storage
            .select_pool_supplier("v0", "chatgpt", &[])
            .await
            .unwrap()
    );
    assert_eq!(route.tag_id.as_deref(), Some("pool"));
    assert!(storage.delete_supplier_tag("pool").await.is_err());
    assert!(
        !storage
            .save_pool_route("v0", "chatgpt", None, Some(0))
            .await
            .unwrap()
    );
    let id = storage
        .select_pool_supplier("v0", "chatgpt", &[])
        .await
        .unwrap()
        .unwrap();
    let revision = storage.supplier_auth_revision(&id).await.unwrap().unwrap();
    storage.reject_supplier_auth(&id, revision).await.unwrap();
    let next = storage
        .select_pool_supplier("v0", "chatgpt", &[])
        .await
        .unwrap()
        .unwrap();
    assert_ne!(next, id);
    storage
        .set_account_status(&next, SupplierStatus::Disabled)
        .await
        .unwrap();
    assert!(
        storage
            .select_pool_supplier("v0", "chatgpt", &[])
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("INSERT INTO providers VALUES('other','Other')")
        .execute(storage.pool())
        .await
        .unwrap();
    storage
        .save_supplier_tag("other", "other", "Main")
        .await
        .unwrap();
    assert!(
        storage
            .edit_supplier_tags(&[id], &["other".into()], false)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cooldown_recovers_by_time_and_membership_removal_clears_bindings() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("cooldown-pg"))
        .await
        .unwrap();
    supplier(&storage, "s").await;
    consumer(&storage, "v").await;
    storage
        .save_supplier_tag("pool", "chatgpt", "Pool")
        .await
        .unwrap();
    storage
        .edit_supplier_tags(&["s".into()], &["pool".into()], false)
        .await
        .unwrap();
    storage
        .save_pool_route("v", "chatgpt", Some("pool"), Some(1))
        .await
        .unwrap();
    let rev = storage.supplier_auth_revision("s").await.unwrap().unwrap();
    let now = chrono::Utc::now().timestamp();
    storage
        .exhaust_supplier_quota("s", rev, now + 600, "usage_limit_reached")
        .await
        .unwrap();
    assert_eq!(
        storage
            .supplier_health("s")
            .await
            .unwrap()
            .cooldown_kind
            .as_deref(),
        Some("quota_exhausted")
    );
    assert!(
        storage
            .select_pool_supplier("v", "chatgpt", &[])
            .await
            .unwrap()
            .is_none()
    );
    sqlx::query("UPDATE supplier_health SET cooldown_until=$1 WHERE account_id='s'")
        .bind(now - 1)
        .execute(storage.pool())
        .await
        .unwrap();
    storage.invalidate_cached_snapshots();
    assert_eq!(
        storage
            .select_pool_supplier("v", "chatgpt", &[])
            .await
            .unwrap()
            .as_deref(),
        Some("s")
    );
    assert!(
        storage
            .supplier_health("s")
            .await
            .unwrap()
            .cooldown_kind
            .is_none()
    );
    storage
        .exhaust_supplier_quota("s", rev - 1, now + 600, "usage_limit_reached")
        .await
        .unwrap();
    assert!(
        storage
            .supplier_health("s")
            .await
            .unwrap()
            .cooldown_kind
            .is_none()
    );
    storage
        .edit_supplier_tags(&["s".into()], &["pool".into()], true)
        .await
        .unwrap();
    let route = storage
        .execution_route("v", "chatgpt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(route.tag_id.as_deref(), Some("pool"));
    assert!(
        storage
            .select_pool_supplier("v", "chatgpt", &[])
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn fresh_quota_recovery_and_manual_reset_repair_routes_without_erasing_newer_failures() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("recovery-pg"))
        .await
        .unwrap();
    supplier(&storage, "s").await;
    supplier(&storage, "other").await;
    consumer(&storage, "v").await;
    storage
        .save_supplier_tag("pool", "chatgpt", "Pool")
        .await
        .unwrap();
    storage
        .edit_supplier_tags(&["s".into()], &["pool".into()], false)
        .await
        .unwrap();
    storage
        .save_pool_route("v", "chatgpt", Some("pool"), Some(1))
        .await
        .unwrap();
    let auth = storage.supplier_auth_revision("s").await.unwrap().unwrap();
    let until = chrono::Utc::now().timestamp() + 86400;
    storage
        .exhaust_supplier_quota("s", auth, until, "usage_limit_reached")
        .await
        .unwrap();
    let observed = storage
        .supplier_health("s")
        .await
        .unwrap()
        .cooldown_revision;
    assert!(
        storage
            .select_pool_supplier("v", "chatgpt", &[])
            .await
            .unwrap()
            .is_none()
    );
    // Identical exhaustion in the same second must still supersede the check.
    storage
        .exhaust_supplier_quota("s", auth, until, "usage_limit_reached")
        .await
        .unwrap();
    assert!(
        !storage
            .recover_supplier_quota("s", auth, observed)
            .await
            .unwrap()
    );
    let current = storage
        .supplier_health("s")
        .await
        .unwrap()
        .cooldown_revision;
    assert!(
        storage
            .recover_supplier_quota("s", auth, current)
            .await
            .unwrap()
    );
    let health = storage.supplier_health("s").await.unwrap();
    assert!(health.cooldown_kind.is_none());
    assert!(health.cooldown_until.is_none());
    assert!(health.cooldown_code.is_none());
    assert_eq!(
        storage
            .select_pool_supplier("v", "chatgpt", &[])
            .await
            .unwrap()
            .as_deref(),
        Some("s")
    );

    storage
        .exhaust_supplier_quota("s", auth, until, "usage_limit_reached")
        .await
        .unwrap();
    let before_reset = storage
        .supplier_health("s")
        .await
        .unwrap()
        .cooldown_revision;
    storage.reset_supplier_availability("s").await.unwrap();
    storage
        .exhaust_supplier_quota_if_unchanged("s", auth, until, "usage_limit_reached", before_reset)
        .await
        .unwrap();
    assert!(
        storage
            .supplier_health("s")
            .await
            .unwrap()
            .cooldown_kind
            .is_none()
    );
    assert_eq!(
        storage
            .select_pool_supplier("v", "chatgpt", &[])
            .await
            .unwrap()
            .as_deref(),
        Some("s")
    );
    // A subsequent real generation can still report exhaustion.
    storage
        .exhaust_supplier_quota("s", auth, until, "usage_limit_reached")
        .await
        .unwrap();
    storage.reject_supplier_auth("s", auth).await.unwrap();
    storage
        .set_account_status("s", SupplierStatus::Disabled)
        .await
        .unwrap();
    storage.reset_supplier_availability("s").await.unwrap();
    assert!(
        storage
            .supplier_health("s")
            .await
            .unwrap()
            .authentication_invalid
    );
    assert_eq!(
        storage.require_account("s").await.unwrap().status,
        SupplierStatus::Disabled
    );
    assert!(
        storage
            .select_pool_supplier("v", "chatgpt", &[])
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .supplier_health("other")
            .await
            .unwrap()
            .cooldown_revision,
        0
    );
}

#[tokio::test]
async fn background_candidates_are_only_currently_exhausted_authorized_enabled_suppliers() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("probes-pg"))
        .await
        .unwrap();
    let now = chrono::Utc::now().timestamp();
    for id in [
        "exhausted",
        "healthy",
        "disabled",
        "rejected",
        "expired",
        "reauthorized",
        "no-token",
    ] {
        supplier(&storage, id).await;
        if id == "healthy" {
            continue;
        }
        let auth = storage.supplier_auth_revision(id).await.unwrap().unwrap();
        storage
            .exhaust_supplier_quota(
                id,
                auth,
                if id == "expired" {
                    now - 1
                } else {
                    now + 86400
                },
                "usage_limit_reached",
            )
            .await
            .unwrap();
        match id {
            "disabled" => {
                storage
                    .set_account_status(id, SupplierStatus::Disabled)
                    .await
                    .unwrap();
            }
            "rejected" => storage.reject_supplier_auth(id, auth).await.unwrap(),
            "reauthorized" => {
                storage
                    .upsert_supplier_tokens(SupplierTokens {
                        account_id: id.into(),
                        access_token: Some("rotated-fixture".into()),
                        ..Default::default()
                    })
                    .await
                    .unwrap();
                assert!(storage.supplier_auth_revision(id).await.unwrap().unwrap() > auth);
                let old = storage.supplier_health(id).await.unwrap().cooldown_revision;
                assert!(!storage.recover_supplier_quota(id, auth, old).await.unwrap());
            }
            "no-token" => {
                sqlx::query("DELETE FROM supplier_tokens WHERE account_id=$1")
                    .bind(id)
                    .execute(storage.pool())
                    .await
                    .unwrap();
            }
            _ => {}
        }
    }
    assert_eq!(
        storage
            .supplier_quota_probe_candidates("chatgpt")
            .await
            .unwrap(),
        vec!["exhausted"]
    );
    assert!(
        storage
            .supplier_quota_probe_candidates("grok")
            .await
            .unwrap()
            .is_empty()
    );
    storage
        .reset_supplier_availability("exhausted")
        .await
        .unwrap();
    assert!(
        storage
            .supplier_quota_probe_candidates("chatgpt")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn rpm_is_atomic_sliding_and_survives_restart_and_honors_policy() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let path = dir.path().join("rpm-pg");
    let storage = codex2api_storage::test_support::open(&path).await.unwrap();
    consumer(&storage, "v").await;
    assert_eq!(
        storage.virtual_rpm_limit("v").await.unwrap().effective_rpm,
        40
    );
    storage.save_virtual_rpm_limit("v", Some(2)).await.unwrap();
    let mut attempts = Vec::new();
    for _ in 0..8 {
        let s = storage.clone();
        attempts.push(tokio::spawn(async move {
            s.admit_virtual_request("v", true).await.unwrap()
        }));
    }
    let mut allowed = 0;
    for a in attempts {
        if a.await.unwrap().is_none() {
            allowed += 1;
        }
    }
    assert_eq!(allowed, 2);
    assert_eq!(
        storage.admit_virtual_request("v", false).await.unwrap(),
        Some(60)
    );
    storage.close().await;
    let storage = codex2api_storage::test_support::open(&path).await.unwrap();
    assert_eq!(
        storage.admit_virtual_request("v", true).await.unwrap(),
        Some(60)
    );
    // Age existing members using the Redis clock instead of injecting an application clock.
    let client = redis::Client::open(std::env::var("CODEX2API_REDIS_URL").unwrap()).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();
    redis::Script::new("local t=redis.call('TIME'); local now=t[1]*1000+math.floor(t[2]/1000); local members=redis.call('ZRANGE',KEYS[1],0,-1); for _,id in ipairs(members) do redis.call('ZADD',KEYS[1],now-60000,id) end; return 1")
        .key(codex2api_storage::test_support::redis_key(&storage,"rpm:v")).invoke_async::<i64>(&mut conn).await.unwrap();
    assert!(
        storage
            .admit_virtual_request("v", true)
            .await
            .unwrap()
            .is_none()
    );
    storage.save_virtual_rpm_limit("v", Some(0)).await.unwrap();
    assert!(
        storage
            .admit_virtual_request("v", true)
            .await
            .unwrap()
            .is_none()
    );
    storage.save_virtual_rpm_limit("v", None).await.unwrap();
    assert_eq!(
        storage.virtual_rpm_limit("v").await.unwrap().effective_rpm,
        40
    );
    let account = storage.platform_account("v").await.unwrap().unwrap();
    let mut plan = storage
        .virtual_plan(&account.plan_id)
        .await
        .unwrap()
        .unwrap();
    plan.config["rpm_limit"] = 1.into();
    storage
        .save_virtual_plan(&plan, Some(plan.revision))
        .await
        .unwrap();
    assert_eq!(
        storage.virtual_rpm_limit("v").await.unwrap().effective_rpm,
        1
    );
    assert_eq!(
        storage.admit_virtual_request("v", true).await.unwrap(),
        Some(60)
    );
}

#[tokio::test]
async fn plan_display_tier_updates_members_without_resetting_subscription_or_billing() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("plans-pg"))
        .await
        .unwrap();
    consumer(&storage, "v").await;
    let before = storage.platform_account("v").await.unwrap().unwrap();
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.plan_type = "prolite".into();
    assert!(
        storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap()
    );
    let after = storage.platform_account("v").await.unwrap().unwrap();
    assert_eq!(after.plan_type.as_str(), "prolite");
    assert_eq!(after.id, before.id);
    assert_eq!(
        after.subscription_expires_at,
        before.subscription_expires_at
    );
    assert_eq!(
        storage.virtual_plan("plus").await.unwrap().unwrap().config,
        plan.config
    );
}

async fn model_candidates(storage: &Storage, model: &str) -> Vec<String> {
    storage
        .pool_candidates("pool", "chatgpt", Some(model))
        .await
        .unwrap()
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect()
}

#[tokio::test]
async fn model_cooldowns_exclude_one_model_until_reset_recovery_or_a_new_login() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("model-cooldown-pg"))
        .await
        .unwrap();
    supplier(&storage, "s").await;
    for model in ["alpha-model", "beta-model"] {
        storage.add_supplier_model("s", model).await.unwrap();
    }
    storage
        .save_supplier_tag("pool", "chatgpt", "Pool")
        .await
        .unwrap();
    storage
        .edit_supplier_tags(&["s".into()], &["pool".into()], false)
        .await
        .unwrap();
    let rev = storage.supplier_auth_revision("s").await.unwrap().unwrap();
    let now = chrono::Utc::now().timestamp();
    storage
        .exhaust_supplier_model_quota("s", rev, "alpha-model", now + 600, "quota_exhausted")
        .await
        .unwrap();
    assert!(model_candidates(&storage, "alpha-model").await.is_empty());
    assert_eq!(model_candidates(&storage, "beta-model").await, vec!["s"]);
    let health = storage.supplier_health("s").await.unwrap();
    assert!(health.cooldown_kind.is_none());
    assert!(health.cooldown_until.is_none());
    let cooldowns = storage.supplier_model_cooldowns("s").await.unwrap();
    assert_eq!(cooldowns.len(), 1);
    assert_eq!(cooldowns[0].model, "alpha-model");
    assert_eq!(cooldowns[0].cooldown_until, now + 600);
    assert_eq!(
        cooldowns[0].cooldown_code.as_deref(),
        Some("quota_exhausted")
    );
    // An observation made with stale credentials is not recorded.
    storage
        .exhaust_supplier_model_quota("s", rev - 1, "beta-model", now + 600, "quota_exhausted")
        .await
        .unwrap();
    assert_eq!(model_candidates(&storage, "beta-model").await, vec!["s"]);
    assert_eq!(
        storage.supplier_model_cooldowns("s").await.unwrap().len(),
        1
    );
    // A fresh official read reporting the model available recovers it before the reset.
    storage
        .recover_supplier_model_quota("s", "alpha-model")
        .await
        .unwrap();
    assert_eq!(model_candidates(&storage, "alpha-model").await, vec!["s"]);
    // The reset time expires the cooldown by itself.
    storage
        .exhaust_supplier_model_quota("s", rev, "alpha-model", now + 600, "quota_exhausted")
        .await
        .unwrap();
    assert!(model_candidates(&storage, "alpha-model").await.is_empty());
    sqlx::query("UPDATE supplier_model_cooldowns SET cooldown_until=$1 WHERE account_id='s'")
        .bind(now - 1)
        .execute(storage.pool())
        .await
        .unwrap();
    storage.invalidate_cached_snapshots();
    assert_eq!(model_candidates(&storage, "alpha-model").await, vec!["s"]);
    assert!(
        storage
            .supplier_model_cooldowns("s")
            .await
            .unwrap()
            .is_empty()
    );
    // A new login discards cooldowns observed with the previous credentials.
    storage
        .exhaust_supplier_model_quota("s", rev, "alpha-model", now + 600, "quota_exhausted")
        .await
        .unwrap();
    supplier(&storage, "s").await;
    assert!(
        storage
            .supplier_model_cooldowns("s")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(model_candidates(&storage, "alpha-model").await, vec!["s"]);
}

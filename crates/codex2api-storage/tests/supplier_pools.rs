use codex2api_storage::{
    NewSupplierAccount, Storage, SupplierStatus, SupplierTokens, VirtualAccount,
};

async fn supplier(storage: &Storage, id: &str) {
    let mut new = NewSupplierAccount::pending_identity(
        id,
        "codex_cli_rs",
        "fixture",
        "Linux",
        "6",
        "x86_64",
        "",
        "{}",
    );
    new.id = Some(id.into());
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
        .save_virtual_account(&VirtualAccount {
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
async fn billing_rejection_survives_refresh_and_quota_recovery_but_not_reauthorization() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("billing.sqlite"))
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
        .save_pool_route("v", "chatgpt", Some("pool"), Some("s"), None)
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
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("billing-reset.sqlite"))
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
async fn obsolete_request_cooldowns_are_ignored_and_migrated_without_clearing_real_failures() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("retired-state.sqlite"))
        .await
        .unwrap();
    for id in ["limited", "exhausted", "unauthorized"] {
        supplier(&storage, id).await;
    }
    for (id, kind) in [
        ("limited", "rate_limited"),
        ("exhausted", "quota_exhausted"),
        ("unauthorized", "rate_limited"),
    ] {
        let revision = storage.supplier_auth_revision(id).await.unwrap().unwrap();
        sqlx::query("INSERT INTO supplier_health(account_id,cooldown_kind,cooldown_until,cooldown_auth_revision,cooldown_code,cooldown_observed_at) VALUES(?,?,?,?,?,?)")
            .bind(id).bind(kind).bind(chrono::Utc::now().timestamp()+600).bind(revision).bind("fixture").bind(chrono::Utc::now().timestamp()).execute(storage.pool()).await.unwrap();
    }
    consumer(&storage, "v").await;
    storage
        .save_supplier_tag("pool", "chatgpt", "Pool")
        .await
        .unwrap();
    storage
        .edit_supplier_tags(&["limited".into()], &["pool".into()], false)
        .await
        .unwrap();
    assert!(
        storage
            .save_pool_route("v", "chatgpt", Some("pool"), Some("limited"), None)
            .await
            .unwrap()
    );
    assert!(
        storage
            .supplier_health("limited")
            .await
            .unwrap()
            .cooldown_kind
            .is_none()
    );
    let revision = storage
        .supplier_auth_revision("unauthorized")
        .await
        .unwrap()
        .unwrap();
    storage
        .reject_supplier_auth("unauthorized", revision)
        .await
        .unwrap();
    sqlx::query(include_str!(
        "../migrations/0052_remove_supplier_throttling.sql"
    ))
    .execute(storage.pool())
    .await
    .unwrap();
    let remaining:i64=sqlx::query_scalar("SELECT COUNT(*) FROM supplier_health WHERE cooldown_kind='rate_limited' OR (account_id!='exhausted' AND cooldown_until IS NOT NULL)").fetch_one(storage.pool()).await.unwrap();
    assert_eq!(remaining, 0);
    assert!(
        storage
            .supplier_health("unauthorized")
            .await
            .unwrap()
            .authentication_invalid
    );
    assert_eq!(
        storage
            .supplier_health("exhausted")
            .await
            .unwrap()
            .cooldown_kind
            .as_deref(),
        Some("quota_exhausted")
    );
    assert_eq!(
        storage
            .select_pool_supplier("v", "chatgpt", &[])
            .await
            .unwrap()
            .as_deref(),
        Some("limited")
    );
}

#[tokio::test]
async fn replacing_tags_is_atomic_scoped_and_preserves_other_account_fields() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("replace-tags.sqlite"))
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
        .save_pool_route("v", "chatgpt", Some("a"), Some("s1"), None)
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
            .execution_route("v", "chatgpt")
            .await
            .unwrap()
            .unwrap()
            .supplier_account_id
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
async fn concurrent_assignments_are_balanced_sticky_and_provider_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("pools.sqlite"))
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
            s.save_pool_route(&owner, "chatgpt", Some("pool"), None, None)
                .await
                .unwrap()
        }));
    }
    for handle in handles {
        assert!(handle.await.unwrap());
    }
    assert_eq!(storage.supplier_binding_count("s1").await.unwrap(), 10);
    assert_eq!(storage.supplier_binding_count("s2").await.unwrap(), 10);
    let before = storage
        .execution_route("v0", "chatgpt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        storage
            .select_pool_supplier("v0", "chatgpt", &[])
            .await
            .unwrap(),
        before.supplier_account_id
    );
    assert!(
        storage
            .save_pool_route(
                "v0",
                "chatgpt",
                Some("pool"),
                Some("outside"),
                Some(before.revision)
            )
            .await
            .is_err()
    );
    assert!(storage.delete_supplier_tag("pool").await.is_err());
    assert!(
        !storage
            .save_pool_route("v0", "chatgpt", None, None, Some(0))
            .await
            .unwrap()
    );
    let id = before.supplier_account_id.unwrap();
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
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("cooldown.sqlite"))
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
        .save_pool_route("v", "chatgpt", Some("pool"), None, None)
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
    sqlx::query("UPDATE supplier_health SET cooldown_until=? WHERE account_id='s'")
        .bind(now - 1)
        .execute(storage.pool())
        .await
        .unwrap();
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
    assert!(route.supplier_account_id.is_none());
}

#[tokio::test]
async fn fresh_quota_recovery_and_manual_reset_repair_routes_without_erasing_newer_failures() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("recovery.sqlite"))
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
        .save_pool_route("v", "chatgpt", Some("pool"), None, None)
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
            .execution_route("v", "chatgpt")
            .await
            .unwrap()
            .unwrap()
            .supplier_account_id
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
            .execution_route("v", "chatgpt")
            .await
            .unwrap()
            .unwrap()
            .supplier_account_id
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
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("probes.sqlite"))
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
                sqlx::query("DELETE FROM supplier_tokens WHERE account_id=?")
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
async fn rpm_is_atomic_sliding_persistent_and_honors_default_override_and_unlimited() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rpm.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    consumer(&storage, "v").await;
    assert_eq!(
        storage.virtual_rpm_limit("v").await.unwrap().effective_rpm,
        20
    );
    storage.save_virtual_rpm_limit("v", Some(2)).await.unwrap();
    let mut attempts = Vec::new();
    for _ in 0..8 {
        let s = storage.clone();
        attempts.push(tokio::spawn(async move {
            s.admit_virtual_request("v", true, 100_000).await.unwrap()
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
        storage
            .admit_virtual_request("v", false, 159_999)
            .await
            .unwrap(),
        Some(1)
    );
    storage.close().await;
    let storage = Storage::open(&path).await.unwrap();
    assert_eq!(
        storage
            .admit_virtual_request("v", true, 159_999)
            .await
            .unwrap(),
        Some(1)
    );
    assert!(
        storage
            .admit_virtual_request("v", true, 160_000)
            .await
            .unwrap()
            .is_none()
    );
    storage.save_virtual_rpm_limit("v", Some(0)).await.unwrap();
    assert!(
        storage
            .admit_virtual_request("v", true, 160_000)
            .await
            .unwrap()
            .is_none()
    );
    storage.save_virtual_rpm_limit("v", None).await.unwrap();
    assert_eq!(
        storage.virtual_rpm_limit("v").await.unwrap().effective_rpm,
        20
    );
    let mut settings = storage.gateway_settings().await.unwrap();
    settings.default_rpm = 1;
    storage.save_gateway_settings(&settings).await.unwrap();
    assert_eq!(
        storage.virtual_rpm_limit("v").await.unwrap().effective_rpm,
        1
    );
    assert_eq!(
        storage
            .admit_virtual_request("v", true, 160_000)
            .await
            .unwrap(),
        Some(60)
    );
}

#[tokio::test]
async fn plan_display_tier_updates_members_without_resetting_subscription_or_billing() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("plans.sqlite"))
        .await
        .unwrap();
    consumer(&storage, "v").await;
    let before = storage.virtual_account("v").await.unwrap().unwrap();
    let mut plan = storage.virtual_plan("plus").await.unwrap().unwrap();
    plan.plan_type = "prolite".into();
    assert!(
        storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap()
    );
    let after = storage.virtual_account("v").await.unwrap().unwrap();
    assert_eq!(after.effective_plan(), "prolite");
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

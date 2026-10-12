use super::*;
use codex2api_storage::{
    NewSupplierAccount, PlatformAccount, Storage, SupplierScheduling, SupplierTokens,
};

mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
use account_fixture::AccountFixture;

async fn fixture() -> (tempfile::TempDir, ApiState, VirtualAccess, ScheduleRequest) {
    fixture_for("chatgpt").await
}

async fn fixture_for(
    provider: &str,
) -> (tempfile::TempDir, ApiState, VirtualAccess, ScheduleRequest) {
    let directory = tempfile::tempdir().unwrap();
    let storage = Storage::open(directory.path().join("scheduler.sqlite"))
        .await
        .unwrap();
    let mut supplier = NewSupplierAccount::pending_identity(
        "installation",
        "codex_cli_rs",
        "fixture",
        "Linux",
        "6",
        "x86_64",
        "",
        "{}",
    );
    supplier.id = Some("supplier".into());
    supplier.provider_id = provider.into();
    supplier.chatgpt_account_id = Some("workspace".into());
    supplier.chatgpt_user_id = Some("user".into());
    storage
        .save_authorized_account(
            supplier,
            SupplierTokens {
                access_token: Some("fixture".into()),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    let plan = if provider == "grok" {
        storage.platform_free_plan(provider).await.unwrap()
    } else {
        storage.virtual_plan("plus").await.unwrap().unwrap()
    };
    let consumer = PlatformAccount {
        id: "owner".into(),
        provider_id: provider.into(),
        username: "owner".into(),
        password_hash: "fixture".into(),
        name: "Owner".into(),
        email: "owner@example.test".into(),
        plan_type: plan.plan_type,
        plan_id: plan.id,
        subscription_expires_at: None,
        enabled: true,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_account_fixture(&consumer).await.unwrap();
    let device = storage
        .create_virtual_device(&consumer, "refresh", &Default::default())
        .await
        .unwrap()
        .unwrap();
    storage
        .register_virtual_access(
            &device,
            "refresh",
            "access",
            chrono::Utc::now().timestamp() + 600,
        )
        .await
        .unwrap();
    let access = storage
        .virtual_access(&codex2api_storage::hash_token("access"))
        .await
        .unwrap()
        .unwrap();
    storage
        .sync_supported_models(&[codex2api_core::SupportedModel {
            provider_id: provider.into(),
            model: "fixture-model".into(),
            kind: "text".into(),
        }])
        .await
        .unwrap();
    storage
        .add_supplier_model("supplier", "fixture-model")
        .await
        .unwrap();
    storage
        .save_supplier_tag("pool", provider, "Pool")
        .await
        .unwrap();
    storage
        .replace_supplier_tags(&["supplier".into()], &["pool".into()])
        .await
        .unwrap();
    storage
        .save_pool_route("owner", provider, Some("pool"), None)
        .await
        .unwrap();
    let mut settings = storage.gateway_settings().await.unwrap();
    settings.scheduler_mode = "traditional".into();
    settings.sticky_escape = false;
    settings.pool_wait_ms = 2000;
    storage.save_gateway_settings(&settings).await.unwrap();
    let accounts = codex2api_accounts::SupplierAccountStore::open(storage.clone());
    let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
    let state = ApiState::new(
        storage,
        accounts,
        codex2api_upstream::UpstreamPool::new(auth),
    );
    let request = ScheduleRequest {
        model: Some("fixture-model".into()),
        session_hash: Some("session".into()),
        ..Default::default()
    };
    (directory, state, access, request)
}

#[tokio::test]
async fn both_providers_share_concurrent_scheduling_and_recheck_stale_model_snapshots() {
    for provider in ["chatgpt", "grok"] {
        for mode in ["traditional", "advanced"] {
            let (_directory, state, access, request) = fixture_for(provider).await;
            let mut settings = state.storage.gateway_settings().await.unwrap();
            settings.scheduler_mode = mode.into();
            state
                .storage
                .save_gateway_settings(&settings)
                .await
                .unwrap();
            let mut jobs = tokio::task::JoinSet::new();
            for _ in 0..3 {
                let (state, access, request) = (state.clone(), access.clone(), request.clone());
                jobs.spawn(async move { select(&state, &access, &request, &[]).await.unwrap() });
            }
            let mut selected = Vec::new();
            while let Some(result) = jobs.join_next().await {
                selected.push(result.unwrap());
            }
            assert_eq!(selected.len(), 3);
            assert!(selected.iter().all(|s| s.account.provider_id == provider));
            assert_eq!(state.storage.supplier_metrics("supplier").active, 3);
            drop(selected);
            assert_eq!(state.storage.supplier_metrics("supplier").active, 0);
            let cached = state
                .storage
                .pool_candidates("pool", provider, request.model.as_deref())
                .await
                .unwrap();
            assert_eq!(cached.len(), 1);
            // Existing readers retain their snapshot; new selections see the model edit.
            state
                .storage
                .remove_supplier_model("supplier", "fixture-model")
                .await
                .unwrap();
            assert!(select(&state, &access, &request, &[]).await.is_err());
            assert_eq!(state.storage.supplier_metrics("supplier").active, 0);
        }
    }
}

#[tokio::test]
async fn load_factor_ranks_without_reducing_concurrency() {
    let (_directory, state, access, request) = fixture().await;
    let policy = state.storage.supplier_scheduling("supplier").await.unwrap();
    assert!(
        state
            .storage
            .save_supplier_scheduling(
                "supplier",
                &SupplierScheduling {
                    concurrency: 5,
                    load_factor: Some(1),
                    ..policy
                }
            )
            .await
            .unwrap()
    );
    let occupied = state
        .storage
        .acquire_supplier("supplier", 5)
        .await
        .unwrap()
        .unwrap();
    let selected = select(&state, &access, &request, &[]).await.unwrap();
    assert_eq!(selected.account.id, "supplier");
    assert_eq!(state.storage.supplier_metrics("supplier").active, 2);
    drop(selected);
    drop(occupied);
    assert_eq!(state.storage.supplier_metrics("supplier").active, 0);
}

#[tokio::test]
async fn queued_requests_recheck_pool_and_token_before_using_a_supplier() {
    for revoke_token in [false, true] {
        let (_directory, state, access, request) = fixture().await;
        let storage = state.storage.clone();
        let policy = storage.supplier_scheduling("supplier").await.unwrap();
        assert!(
            storage
                .save_supplier_scheduling(
                    "supplier",
                    &SupplierScheduling {
                        concurrency: 1,
                        ..policy
                    }
                )
                .await
                .unwrap()
        );
        storage
            .save_routing_affinity("sticky_session:chatgpt:pool:owner:session", "supplier", 60)
            .await
            .unwrap();
        let occupied = storage
            .acquire_supplier("supplier", 1)
            .await
            .unwrap()
            .unwrap();
        let token_hash = access.token_hash.clone();
        let selecting = tokio::spawn(async move { select(&state, &access, &request, &[]).await });
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while storage.supplier_metrics("supplier").waiting == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        if revoke_token {
            sqlx::query("DELETE FROM virtual_access_tokens WHERE token_hash=?")
                .bind(token_hash)
                .execute(storage.pool())
                .await
                .unwrap();
        } else {
            let route = storage
                .execution_route("owner", "chatgpt")
                .await
                .unwrap()
                .unwrap();
            assert!(
                storage
                    .save_pool_route("owner", "chatgpt", None, Some(route.revision))
                    .await
                    .unwrap()
            );
        }
        drop(occupied);
        assert!(selecting.await.unwrap().is_err());
        assert_eq!(storage.supplier_metrics("supplier").active, 0);
        assert_eq!(storage.supplier_metrics("supplier").waiting, 0);
    }
}

//! OAuth execution through the shared executor. The listener selects and admits
//! the first supplier under its subscription policy; the executor converts, sends,
//! judges and switches from there.
#[cfg(test)]
mod account_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../codex2api-storage/test-support/accounts.rs"
    ));
}
use crate::{OAuthError, OAuthState, Result};
#[cfg(test)]
use account_fixture::AccountFixture;
use codex2api_storage::VirtualAccess;
pub(crate) use codex2api_upstream::scheduler::SupplierContext;
use codex2api_upstream::{
    executor::{Executed, Execution},
    inbound::InboundRequest,
    scheduler::SupplierRoute,
};

pub(crate) fn exhausted() -> OAuthError {
    OAuthError::openai(
        http::StatusCode::SERVICE_UNAVAILABLE,
        "server_error",
        "No supplier is available for this request. Please retry later.",
        Some("no_available_supplier"),
    )
}

pub(crate) async fn select_for(
    state: &OAuthState,
    oauth: &VirtualAccess,
    request: &crate::pool_scheduler::ScheduleRequest,
    excluded: &[String],
) -> Result<SupplierContext> {
    let admitted = excluded.is_empty()
        && request.admission == Some(state.storage.admission_version(&oauth.virtual_account_id));
    if !admitted
        && !state
            .storage
            .virtual_access_valid(&oauth.token_hash)
            .await?
    {
        return Err(OAuthError::invalid_token());
    }
    if !admitted && request.model.is_some() {
        crate::admission::ExecutionService::new(state.storage.clone())
            .authorize(
                &oauth.virtual_account_id,
                &oauth.provider_id,
                crate::admission::ExecutionRequest {
                    kind: codex2api_common::ExecutionKind::InspectModel,
                    model: request.model.as_deref(),
                    service_tier: None,
                    image_size: None,
                },
            )
            .await?;
    }
    crate::pool_scheduler::select(state, oauth, request, excluded).await
}

/// The subscription's execution route as a scheduler route.
pub(crate) async fn route(state: &OAuthState, oauth: &VirtualAccess) -> Result<SupplierRoute> {
    let route = state
        .storage
        .execution_route(&oauth.virtual_account_id, &oauth.provider_id)
        .await?
        .filter(|route| !route.tag_ids.is_empty())
        .ok_or_else(exhausted)?;
    Ok(SupplierRoute {
        owner_id: oauth.virtual_account_id.clone(),
        scope: oauth.provider_id.clone(),
        provider_id: Some(oauth.provider_id.clone()),
        tag_ids: route.tag_ids,
    })
}

/// Run one request on the already selected supplier, switching as the channels'
/// verdicts direct. The reply's prelude passed inspection; its supplier permit is
/// in `context` when the executor selected a replacement.
pub(crate) async fn execute(
    state: &OAuthState,
    oauth: &VirtualAccess,
    initial: SupplierContext,
    input: InboundRequest,
    log: &mut Option<crate::usage::RequestLog>,
) -> Result<Executed> {
    let route = route(state, oauth).await?;
    let revision = state
        .storage
        .execution_route(&oauth.virtual_account_id, &oauth.provider_id)
        .await?
        .map(|route| route.revision);
    let schedule = initial.request.clone();
    let turn_owner = initial.reused_affinity.then(|| initial.account.id.clone());
    let mut ledger = crate::usage::Ledger(log);
    let execution = Execution {
        storage: &state.storage,
        pool: &state.upstream,
        route: &route,
        schedule,
        inbound: &input,
        ledger: &mut ledger,
        excluded: Vec::new(),
        headers_for: Some(Box::new(move |account, headers| {
            if turn_owner.as_deref() != Some(account.id.as_str()) {
                headers.remove("x-codex-turn-state");
            }
        })),
        initial: Some(initial),
    };
    let result = execution
        .run(|| async {
            state
                .storage
                .virtual_access(&oauth.token_hash)
                .await?
                .ok_or(codex2api_upstream::scheduler::SchedulerError::ConsumerInvalid)?;
            let current = state
                .storage
                .execution_route(&oauth.virtual_account_id, &oauth.provider_id)
                .await?;
            if !current.is_some_and(|current| {
                current.tag_ids == route.tag_ids && Some(current.revision) == revision
            }) {
                return Err(codex2api_upstream::scheduler::exhausted());
            }
            Ok(())
        })
        .await;
    if let Err(error) = &result
        && let Some(log) = log
    {
        log.settle_failure(error).await?;
    }
    result.map_err(OAuthError::executor)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use codex2api_storage::{
        OAuthDeviceIdentity, PlatformAccount, SupplierAccountUpdate, SupplierStatus,
    };
    use codex2api_suppliers::{AuthDotJson, SupplierAccountStore, TokenData};

    pub(crate) async fn setup_pool() -> (
        codex2api_storage::test_support::TestDirectory,
        OAuthState,
        VirtualAccess,
        Vec<String>,
    ) {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("http-pool-pg"))
            .await
            .unwrap();
        storage
            .sync_supported_models(&codex2api_upstream::supported_models())
            .await
            .unwrap();
        let mut settings = storage.gateway_settings().await.unwrap();
        settings.scheduler_mode = "traditional".into();
        storage.save_gateway_settings(&settings).await.unwrap();
        let accounts = SupplierAccountStore::open(storage.clone());
        let mut ids = Vec::new();
        for _ in 0..2 {
            let a = accounts.create_pending().await.unwrap().account;
            storage
                .update_account(
                    &a.id,
                    SupplierAccountUpdate {
                        status: Some(SupplierStatus::Active),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            accounts
                .save_auth_for_account(
                    &a.id,
                    &AuthDotJson::chatgpt(
                        TokenData {
                            id_token: "fixture".into(),
                            access_token: "fixture".into(),
                            refresh_token: "fixture".into(),
                            account_id: Some(a.id.clone()),
                        },
                        None,
                    ),
                )
                .await
                .unwrap();
            storage.add_supplier_model(&a.id, "gpt-5.5").await.unwrap();
            storage
                .save_supplier_scheduling(
                    &a.id,
                    &codex2api_storage::SupplierScheduling {
                        priority: ids.len() as i32,
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            ids.push(a.id);
        }
        let owner = PlatformAccount {
            provider_id: "chatgpt".into(),
            id: "consumer".into(),
            username: "consumer".into(),
            password_hash: "fixture".into(),
            name: "Consumer".into(),
            email: "consumer@example.test".into(),
            plan_type: "plus".into(),
            plan_id: "plus".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        storage.save_account_fixture(&owner).await.unwrap();
        storage
            .save_supplier_tag("pool", "chatgpt", "Pool")
            .await
            .unwrap();
        storage
            .edit_supplier_tags(&ids, &["pool".into()], false)
            .await
            .unwrap();
        storage
            .save_pool_route(&owner.id, "chatgpt", Some(&["pool".into()]), Some(1))
            .await
            .unwrap();
        let device = storage
            .create_virtual_device(&owner, "refresh", &OAuthDeviceIdentity::default())
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
        let oauth = storage
            .virtual_access(&codex2api_storage::hash_token("access"))
            .await
            .unwrap()
            .unwrap();
        let auth = codex2api_suppliers::AuthService::new(accounts.clone()).unwrap();
        let state = OAuthState::new(
            storage.clone(),
            accounts,
            codex2api_upstream::UpstreamPool::new(auth),
        );
        (dir, state, oauth, ids)
    }
}

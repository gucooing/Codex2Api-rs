//! OAuth authorization around the provider-neutral supplier scheduler.
use crate::{OAuthError, OAuthState, Result};
use codex2api_storage::VirtualAccess;
pub(crate) use codex2api_upstream::scheduler::{
    ScheduleRequest, body_seed, hash, header_seed, previous_key, text,
};
#[cfg(test)]
#[path = "pool_scheduler_tests.rs"]
mod tests;

pub(crate) fn context_required() -> OAuthError {
    codex2api_upstream::scheduler::context_required().into()
}

pub(crate) async fn select(
    state: &OAuthState,
    oauth: &VirtualAccess,
    request: &ScheduleRequest,
    excluded: &[String],
) -> Result<crate::pool_execution::SupplierContext> {
    let route = state
        .storage
        .execution_route(&oauth.virtual_account_id, &oauth.provider_id)
        .await?
        .filter(|route| !route.tag_ids.is_empty())
        .ok_or_else(crate::pool_execution::exhausted)?;
    // A subscription executes only on suppliers of its own client platform.
    let target = codex2api_upstream::scheduler::SupplierRoute {
        owner_id: oauth.virtual_account_id.clone(),
        scope: oauth.provider_id.clone(),
        provider_id: Some(oauth.provider_id.clone()),
        tag_ids: route.tag_ids.clone(),
    };
    let result = codex2api_upstream::scheduler::select(
        &state.storage,
        &target,
        request,
        excluded,
        || async {
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
                current.tag_ids == route.tag_ids && current.revision == route.revision
            }) {
                return Err(codex2api_upstream::scheduler::exhausted());
            }
            Ok(())
        },
    )
    .await?;
    Ok(result)
}

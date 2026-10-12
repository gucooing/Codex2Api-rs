//! OAuth authorization around the provider-neutral supplier scheduler.
use crate::{ApiError, ApiState, Result};
pub(crate) use codex2api_scheduler::{
    ScheduleRequest, body_seed, hash, header_seed, previous_key, text,
};
use codex2api_storage::VirtualAccess;
#[cfg(test)]
#[path = "pool_scheduler_tests.rs"]
mod tests;

pub(crate) fn context_required() -> ApiError {
    codex2api_scheduler::context_required().into()
}

pub(crate) async fn select(
    state: &ApiState,
    oauth: &VirtualAccess,
    request: &ScheduleRequest,
    excluded: &[String],
) -> Result<crate::pool_execution::SupplierContext> {
    let route = state
        .storage
        .execution_route(&oauth.virtual_account_id, &oauth.provider_id)
        .await?
        .ok_or_else(crate::pool_execution::exhausted)?;
    let tag = route.tag_id.ok_or_else(crate::pool_execution::exhausted)?;
    let target = codex2api_scheduler::SupplierRoute {
        owner_id: oauth.virtual_account_id.clone(),
        provider_id: oauth.provider_id.clone(),
        tag_ids: vec![tag.clone()],
    };
    let result =
        codex2api_scheduler::select(&state.storage, &target, request, excluded, || async {
            state
                .storage
                .virtual_access(&oauth.token_hash)
                .await?
                .ok_or(codex2api_scheduler::SchedulerError::ConsumerInvalid)?;
            let current = state
                .storage
                .execution_route(&oauth.virtual_account_id, &oauth.provider_id)
                .await?;
            if !current.is_some_and(|current| {
                current.tag_id.as_deref() == Some(tag.as_str())
                    && current.revision == route.revision
            }) {
                return Err(codex2api_scheduler::exhausted());
            }
            Ok(())
        })
        .await?;
    Ok(result)
}

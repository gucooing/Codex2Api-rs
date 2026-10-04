use super::error::ApiResult;
use crate::AdminState;
use axum::{
    Json,
    extract::{Query, State},
};
use codex2api_storage::WalletEntryFilter;

pub async fn list(
    State(state): State<AdminState>,
    Query(filter): Query<WalletEntryFilter>,
) -> ApiResult {
    Ok(Json(state.storage.admin_wallet_entries(&filter).await?))
}

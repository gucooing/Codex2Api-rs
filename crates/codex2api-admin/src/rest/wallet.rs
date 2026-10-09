use crate::{AdminState, rest::error::ApiResult};
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use codex2api_storage::WalletEntryFilter;

pub(super) fn router() -> Router<AdminState> {
    Router::new().route("/wallet-entries", get(list))
}

pub async fn list(
    State(state): State<AdminState>,
    Query(filter): Query<WalletEntryFilter>,
) -> ApiResult {
    Ok(Json(state.storage.admin_wallet_entries(&filter).await?))
}

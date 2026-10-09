use crate::{
    UserState,
    error::{Result, UserError},
};
use axum::{
    Json,
    extract::{Extension, Query, State},
};
use codex2api_storage::UserSession;
use serde_json::Value;

pub(crate) async fn wallet(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Query(query): Query<codex2api_storage::ListQuery>,
) -> Result<Json<Value>> {
    let user = state
        .storage
        .user(&session.user_id)
        .await?
        .ok_or_else(UserError::unauthorized)?;
    let mut page = serde_json::to_value(state.storage.wallet_entries(&user.id, &query).await?)
        .map_err(codex2api_storage::StorageError::from)?;
    page["currency"] = "USD".into();
    page["balance_usd"] = codex2api_storage::format_units(user.wallet_cents, 2).into();
    Ok(Json(page))
}

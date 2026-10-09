use crate::{
    UserState,
    error::{Result, UserError},
};
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use codex2api_storage::UserSession;
use serde_json::{Value, json};

pub(crate) async fn devices(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Query(query): Query<codex2api_storage::ListQuery>,
) -> Result<Json<Value>> {
    Ok(Json(json!(
        state.storage.devices(&session.user_id, &query).await?
    )))
}

pub(crate) async fn revoke_device(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    if !state.storage.revoke_device(&session.user_id, &id).await? {
        return Err(UserError::missing());
    }
    Ok(Json(json!({"ok":true})))
}

use crate::{UserState, error::Result};
use axum::{
    Json,
    extract::{Extension, Query, State},
};
use codex2api_storage::{UserSession, UserUsageFilter};
use serde_json::Value;

pub(crate) async fn usage(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Query(filter): Query<UserUsageFilter>,
) -> Result<Json<Value>> {
    Ok(Json(state.storage.usage(&session.user_id, &filter).await?))
}

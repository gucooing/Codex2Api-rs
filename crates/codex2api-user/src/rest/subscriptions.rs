use crate::{UserState, error::Result};
use axum::{
    Json,
    extract::{Extension, Query, State},
};
use codex2api_storage::UserSession;
use serde_json::{Value, json};

pub(crate) async fn subscriptions(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Query(query): Query<codex2api_storage::ListQuery>,
) -> Result<Json<Value>> {
    Ok(Json(json!(
        state
            .storage
            .subscriptions(&session.user_id, &query)
            .await?
    )))
}

pub(crate) async fn plans(
    State(state): State<UserState>,
    Query(query): Query<codex2api_storage::ListQuery>,
) -> Result<Json<Value>> {
    Ok(Json(json!(state.storage.plans(&query).await?)))
}

//! User-owned API keys and the public model/pricing catalog.
use crate::{
    UserState,
    error::{Result, UserError},
};
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use codex2api_storage::{ApiListQuery, UserSession};
use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) async fn groups(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
) -> Result<Json<Value>> {
    let origin = state
        .storage
        .public_metered_api_url(&state.metered_api_base_url)
        .await?;
    Ok(Json(json!({
        "items": state.storage.api_groups(&session.user_id).await?,
        "api_base_url": format!("{origin}/v1"),
    })))
}

pub(crate) async fn market(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Query(query): Query<ApiListQuery>,
) -> Result<Json<Value>> {
    Ok(Json(
        state.storage.api_catalog(&session.user_id, &query).await?,
    ))
}

pub(crate) async fn keys(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Query(query): Query<ApiListQuery>,
) -> Result<Json<Value>> {
    let page = state
        .storage
        .api_keys(&session.user_id, &query)
        .await?
        .map(|key| {
            let active = (state.api_concurrency)(&key.id);
            let mut value = json!(key);
            value["active_requests"] = active.into();
            value
        });
    Ok(Json(json!(page)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateKey {
    name: String,
    group_id: String,
}

pub(crate) async fn create_key(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Json(input): Json<CreateKey>,
) -> Result<Json<Value>> {
    let created = state
        .storage
        .create_api_key(&session.user_id, &input.name, &input.group_id)
        .await?;
    let active = (state.api_concurrency)(&created.key.id);
    let mut value = json!(created);
    value["key"]["active_requests"] = active.into();
    Ok(Json(value))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateKey {
    name: String,
    enabled: bool,
    revision: i64,
}

pub(crate) async fn update_key(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Path(id): Path<String>,
    Json(input): Json<UpdateKey>,
) -> Result<Json<Value>> {
    if !state
        .storage
        .update_api_key(
            &session.user_id,
            &id,
            &input.name,
            input.enabled,
            input.revision,
        )
        .await?
    {
        return Err(UserError::from(
            codex2api_storage::StorageError::ProxyChanged,
        ));
    }
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Revision {
    revision: i64,
}

pub(crate) async fn revoke_key(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Path(id): Path<String>,
    Json(input): Json<Revision>,
) -> Result<Json<Value>> {
    if !state
        .storage
        .revoke_api_key(&session.user_id, &id, input.revision)
        .await?
    {
        return Err(UserError::from(
            codex2api_storage::StorageError::ProxyChanged,
        ));
    }
    Ok(Json(json!({"ok":true})))
}

pub(crate) async fn usage(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Query(query): Query<ApiListQuery>,
) -> Result<Json<Value>> {
    Ok(Json(
        state.storage.api_usage(&session.user_id, &query).await?,
    ))
}

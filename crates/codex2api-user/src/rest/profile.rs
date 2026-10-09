use crate::{
    UserState,
    error::{Result, UserError},
};
use axum::{
    Json,
    extract::{Extension, State},
};
use codex2api_storage::UserSession;
use serde::Deserialize;
use serde_json::{Value, json};

pub(crate) async fn session(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
) -> Result<Json<Value>> {
    let user = state
        .storage
        .user(&session.user_id)
        .await?
        .filter(|u| u.enabled)
        .ok_or_else(UserError::unauthorized)?;
    Ok(Json(
        json!({"user":user.view(),"csrf_token":session.csrf_token}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Password {
    current_password: String,
    new_password: String,
}

pub(crate) async fn password(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Json(input): Json<Password>,
) -> Result<Json<Value>> {
    if input.new_password.is_empty() || input.new_password.len() > 1024 {
        return Err(UserError::bad("请填写新密码，最多 1024 字节"));
    }
    let user = state
        .storage
        .user(&session.user_id)
        .await?
        .ok_or_else(UserError::unauthorized)?;
    crate::auth::verify_user(&state, &user.username, input.current_password).await?;
    let password_hash =
        tokio::task::spawn_blocking(move || codex2api_storage::hash_password(&input.new_password))
            .await
            .map_err(|_| UserError::bad("密码保存失败"))??;
    if !state.storage.change_password(&user, password_hash).await? {
        return Err(codex2api_storage::StorageError::ProxyChanged.into());
    }
    Ok(Json(json!({"ok":true})))
}

use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use codex2api_storage::User;
use serde::Deserialize;
use serde_json::json;

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .route("/users", get(list).post(create))
        .route("/users/options", get(options))
        .route("/users/{id}/wallet-adjustments", post(adjust_wallet))
        .route("/users/{id}/wallet-entries", get(wallet_entries))
        .route("/users/{id}", get(detail).put(update))
}

pub async fn list(
    State(state): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    Ok(Json(json!(
        state.storage.user_page(&q).await?.map(|u| u.view())
    )))
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct UserOptionsQuery {
    search: String,
}

pub async fn options(
    State(state): State<AdminState>,
    Query(query): Query<UserOptionsQuery>,
) -> ApiResult {
    Ok(Json(state.storage.user_options(&query.search).await?))
}

pub async fn detail(State(state): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    let user = state
        .storage
        .user(&id)
        .await?
        .ok_or_else(ApiError::missing)?;
    Ok(Json(
        json!({"user":user.view(),"subscriptions":state.storage.user_subscriptions(Some(&id),true).await?}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserInput {
    username: String,
    name: String,
    email: String,
    password: String,
    enabled: bool,
    revision: Option<i64>,
}

pub async fn create(State(state): State<AdminState>, Json(input): Json<UserInput>) -> ApiResult {
    save(state, None, input).await
}

pub async fn update(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<UserInput>,
) -> ApiResult {
    save(state, Some(id), input).await
}

async fn save(state: AdminState, id: Option<String>, input: UserInput) -> ApiResult {
    let previous = match &id {
        Some(id) => Some(
            state
                .storage
                .user(id)
                .await?
                .ok_or_else(ApiError::missing)?,
        ),
        None => None,
    };
    if previous.as_ref().map(|u| u.revision) != input.revision {
        return Err(ApiError::conflict());
    }
    let password_hash = if input.password.is_empty() {
        previous
            .as_ref()
            .ok_or_else(|| ApiError::bad("新用户必须设置密码"))?
            .password_hash
            .clone()
    } else {
        if input.password.len() > 1024 {
            return Err(ApiError::bad("密码最多 1024 字节"));
        }
        tokio::task::spawn_blocking(move || codex2api_storage::hash_password(&input.password))
            .await
            .map_err(|_| ApiError::bad("密码保存失败"))??
    };
    let user = User {
        kind: codex2api_storage::UserKind::Regular,
        id: id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        username: input.username,
        name: input.name,
        email: input.email,
        password_hash,
        enabled: input.enabled,
        wallet_cents: 0,
        revision: input.revision.unwrap_or(1),
        created_at: previous
            .map(|u| u.created_at)
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
    };
    if !state.storage.save_user(&user, input.revision).await? {
        return Err(ApiError::conflict());
    }
    Ok(Json(json!(
        state
            .storage
            .user(&user.id)
            .await?
            .ok_or_else(ApiError::missing)?
            .view()
    )))
}

pub async fn adjust_wallet(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<codex2api_storage::WalletAdjustment>,
) -> ApiResult {
    let administrator = state.storage.require_admin_user().await?;
    let entry = state
        .storage
        .adjust_user_wallet(&id, administrator.id, input)
        .await?;
    Ok(Json(json!({"entry":entry})))
}

pub async fn wallet_entries(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    state
        .storage
        .user(&id)
        .await?
        .ok_or_else(ApiError::missing)?;
    Ok(Json(json!(state.storage.wallet_entry_page(&id, &q).await?)))
}

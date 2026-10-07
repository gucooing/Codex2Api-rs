use super::error::{ApiError, ApiResult, ok};
use crate::AdminState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use codex2api_storage::{SubscriptionChange, User};
use serde::Deserialize;
use serde_json::json;

pub async fn list(State(state): State<AdminState>) -> ApiResult {
    Ok(Json(
        json!({"items":state.storage.users().await?.iter().map(User::view).collect::<Vec<_>>()}),
    ))
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
pub async fn detail(State(state): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    let user = state
        .storage
        .user(&id)
        .await?
        .ok_or_else(ApiError::missing)?;
    Ok(Json(
        json!({"user":user.view(),"subscriptions":state.storage.user_subscriptions(Some(&id),true).await?,"wallet_entries":state.storage.wallet_entries(&id).await?}),
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
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct SubscriptionQuery {
    user_id: Option<String>,
    plan_id: Option<String>,
    include_expired: bool,
}
pub async fn subscriptions(
    State(state): State<AdminState>,
    Query(query): Query<SubscriptionQuery>,
) -> ApiResult {
    let items = state
        .storage
        .filter_user_subscriptions(
            query.user_id.as_deref(),
            query.plan_id.as_deref(),
            query.include_expired,
        )
        .await?
        .into_iter()
        .map(|s| {
            let expired = s.expired();
            let mut value = json!(s);
            value["expired"] = expired.into();
            value
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionInput {
    reissue: bool,
    user_id: String,
    plan_id: String,
    expires_at: Option<String>,
    enabled: bool,
    revision: Option<i64>,
}
pub async fn grant(
    State(state): State<AdminState>,
    Json(input): Json<SubscriptionInput>,
) -> ApiResult {
    if input.revision.is_some() {
        return Err(ApiError::bad("新增订阅不能指定已有版本"));
    }
    save_subscription(state, input).await
}
pub async fn update_subscription(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<SubscriptionInput>,
) -> ApiResult {
    let current = state
        .storage
        .user_subscriptions(Some(&input.user_id), true)
        .await?
        .into_iter()
        .find(|s| s.virtual_account_id == id)
        .ok_or_else(ApiError::missing)?;
    let plan = state
        .storage
        .virtual_plan(&input.plan_id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if current.provider_id != plan.provider_id || input.revision != Some(current.revision) {
        return Err(ApiError::conflict());
    }
    save_subscription(state, input).await
}
async fn save_subscription(state: AdminState, input: SubscriptionInput) -> ApiResult {
    state
        .storage
        .save_user_subscription(SubscriptionChange {
            reissue: input.reissue,
            user_id: &input.user_id,
            plan_id: &input.plan_id,
            expires_at: input.expires_at.as_deref(),
            enabled: input.enabled,
            revision: input.revision,
        })
        .await?;
    Ok(ok())
}

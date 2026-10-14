use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult, ok},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use codex2api_storage::SubscriptionChange;
use serde::Deserialize;

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .merge(super::platform_accounts::router("/subscriptions"))
        .route(
            "/subscriptions/{id}",
            get(super::platform_accounts::detail)
                .put(update_subscription)
                .delete(delete_subscription),
        )
        .route(
            "/subscriptions/options",
            get(super::subscription_accounts::options),
        )
        .route(
            "/subscriptions/batch",
            post(super::subscription_accounts::batch),
        )
        .route(
            "/subscriptions/{id}/routing",
            get(super::platform_accounts::routes).put(super::platform_accounts::save_route),
        )
        .route(
            "/subscriptions",
            get(super::subscription_accounts::list).post(grant),
        )
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
    save_subscription(state, None, input).await
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
    save_subscription(state, Some(&id), input).await
}

async fn save_subscription(
    state: AdminState,
    id: Option<&str>,
    input: SubscriptionInput,
) -> ApiResult {
    let id = state
        .storage
        .save_user_subscription(SubscriptionChange {
            subscription_id: id,
            reissue: input.reissue,
            user_id: &input.user_id,
            plan_id: &input.plan_id,
            expires_at: input.expires_at.as_deref(),
            enabled: input.enabled,
            revision: input.revision,
        })
        .await?;
    Ok(Json(
        super::platform_accounts::dto(
            &state,
            &super::platform_accounts::require(&state, &id).await?,
        )
        .await?,
    ))
}
pub async fn delete_subscription(
    State(state): State<AdminState>,
    Path(id): Path<String>,
) -> ApiResult {
    state.storage.delete_user_subscription(&id).await?;
    Ok(ok())
}

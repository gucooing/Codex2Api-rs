use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult, ok},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use codex2api_storage::SubscriptionChange;
use serde::Deserialize;
use serde_json::json;

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .merge(super::platform_accounts::router("/subscriptions"))
        .route(
            "/subscriptions/{id}",
            get(super::platform_accounts::detail).put(update_subscription),
        )
        .route("/subscriptions", get(subscriptions).post(grant))
}

pub async fn subscriptions(
    State(state): State<AdminState>,
    Query(query): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    Ok(Json(json!(
        state.storage.subscription_page(&query).await?.map(|s| {
            let expired = s.expired();
            let mut value = json!(s);
            value["expired"] = expired.into();
            value
        })
    )))
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

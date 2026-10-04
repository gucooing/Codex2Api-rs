use crate::{
    UserState,
    error::{Result, UserError},
};
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use codex2api_storage::{CheckoutInput, OrderFilter, OrderRequest, UserSession, UserUsageFilter};
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
pub(crate) async fn subscriptions(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
) -> Result<Json<Value>> {
    Ok(Json(
        json!({"items":state.storage.subscriptions(&session.user_id).await?}),
    ))
}
pub(crate) async fn plans(State(state): State<UserState>) -> Result<Json<Value>> {
    Ok(Json(json!({"items":state.storage.plans().await?})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Buy {
    preview_token: String,
}
pub(crate) async fn preview(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Json(input): Json<CheckoutInput>,
) -> Result<Json<Value>> {
    Ok(Json(
        state
            .storage
            .checkout_preview(&session.user_id, input)
            .await?,
    ))
}
pub(crate) async fn usage(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Query(filter): Query<UserUsageFilter>,
) -> Result<Json<Value>> {
    Ok(Json(state.storage.usage(&session.user_id, &filter).await?))
}
pub(crate) async fn create_order(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Json(input): Json<Buy>,
) -> Result<Json<Value>> {
    let order = state
        .storage
        .create_order(OrderRequest {
            user_id: &session.user_id,
            preview_token: &input.preview_token,
        })
        .await?;
    Ok(Json(order.view(false)?))
}
pub(crate) async fn orders(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Query(filter): Query<OrderFilter>,
) -> Result<Json<Value>> {
    Ok(Json(state.storage.orders(&session.user_id, &filter).await?))
}
pub(crate) async fn order(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    Ok(Json(
        state
            .storage
            .order(&session.user_id, &id)
            .await?
            .ok_or_else(UserError::missing)?
            .view(false)?,
    ))
}
pub(crate) async fn order_plans(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
) -> Result<Json<Value>> {
    Ok(Json(
        json!({"items":state.storage.order_plans(&session.user_id).await?}),
    ))
}
pub(crate) async fn pay_order(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    Ok(Json(
        state
            .storage
            .pay_order(&session.user_id, &id)
            .await?
            .view(false)?,
    ))
}
pub(crate) async fn cancel_order(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    Ok(Json(
        state
            .storage
            .cancel_order(&session.user_id, &id)
            .await?
            .view(false)?,
    ))
}
pub(crate) async fn wallet(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
) -> Result<Json<Value>> {
    let user = state
        .storage
        .user(&session.user_id)
        .await?
        .ok_or_else(UserError::unauthorized)?;
    Ok(Json(
        json!({"currency":"USD","balance_usd":codex2api_storage::format_units(user.wallet_cents,2),"items":state.storage.wallet_entries(&user.id).await?}),
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
    super::auth::verify_user(&state, &user.username, input.current_password).await?;
    let password_hash =
        tokio::task::spawn_blocking(move || codex2api_storage::hash_password(&input.new_password))
            .await
            .map_err(|_| UserError::bad("密码保存失败"))??;
    if !state.storage.change_password(&user, password_hash).await? {
        return Err(codex2api_storage::StorageError::ProxyChanged.into());
    }
    Ok(Json(json!({"ok":true})))
}
pub(crate) async fn devices(
    State(state): State<UserState>,
    Extension(session): Extension<UserSession>,
) -> Result<Json<Value>> {
    Ok(Json(
        json!({"items":state.storage.devices(&session.user_id).await?}),
    ))
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

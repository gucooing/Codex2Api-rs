use crate::{
    UserState,
    error::{Result, UserError},
};
use axum::{
    Json,
    extract::{Extension, Path, Query, State},
};
use codex2api_storage::{CheckoutInput, OrderFilter, OrderRequest, UserSession};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Buy {
    preview_token: String,
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

use super::error::ApiResult;
use crate::AdminState;
use axum::{
    Json,
    extract::{Path, State},
};
use codex2api_storage::CouponInput;

pub async fn list(State(state): State<AdminState>) -> ApiResult {
    Ok(Json(state.storage.coupons().await?))
}
pub async fn create(State(state): State<AdminState>, Json(input): Json<CouponInput>) -> ApiResult {
    Ok(Json(
        serde_json::to_value(state.storage.save_coupon(None, input).await?)
            .map_err(codex2api_storage::StorageError::from)?,
    ))
}
pub async fn update(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<CouponInput>,
) -> ApiResult {
    Ok(Json(
        serde_json::to_value(state.storage.save_coupon(Some(&id), input).await?)
            .map_err(codex2api_storage::StorageError::from)?,
    ))
}

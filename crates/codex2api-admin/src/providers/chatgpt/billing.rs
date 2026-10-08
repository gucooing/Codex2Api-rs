use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult},
};
use axum::{Json, extract::State};
use codex2api_core::{CHATGPT, providers::chatgpt::SEARCH_OPERATION};
use codex2api_storage::{OperationPrice, decimal_units, format_units};
use serde::Deserialize;
use serde_json::json;

pub(crate) async fn search_price(State(state): State<AdminState>) -> ApiResult {
    let price = state
        .storage
        .operation_price(CHATGPT, SEARCH_OPERATION)
        .await?;
    Ok(Json(json!({
        "price": price.as_ref().and_then(|p| p.price_nano_usd).map(|p| format_units(p, 9)),
        "revision": price.map(|p| p.revision),
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchPriceInput {
    price: Option<String>,
    revision: Option<i64>,
}

pub(crate) async fn save_search_price(
    State(state): State<AdminState>,
    Json(input): Json<SearchPriceInput>,
) -> ApiResult {
    let price = OperationPrice {
        provider_id: CHATGPT.into(),
        operation: SEARCH_OPERATION.into(),
        price_nano_usd: input
            .price
            .as_deref()
            .map(|text| {
                decimal_units(text, 9)
                    .ok_or_else(|| ApiError::bad("每次价格须为非负美元金额，最多九位小数"))
            })
            .transpose()?,
        revision: input.revision.unwrap_or(0),
    };
    if !state
        .storage
        .save_operation_price(&price, input.revision)
        .await?
    {
        return Err(ApiError::conflict());
    }
    search_price(State(state)).await
}

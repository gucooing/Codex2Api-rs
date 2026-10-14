use crate::{
    AdminState,
    rest::{
        error::{ApiError, ApiResult},
        platform_accounts::dto,
    },
};
use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchInput {
    request_id: Option<String>,
    operation: String,
    #[serde(default)]
    ids: Vec<String>,
    #[serde(default)]
    all_matching: bool,
    #[serde(default)]
    filters: codex2api_storage::SubscriptionFilters,
    #[serde(default)]
    excluded_ids: Vec<String>,
    quantity: Option<i64>,
    note: Option<String>,
    activate_at: Option<chrono::DateTime<chrono::Utc>>,
    duration_days: Option<i64>,
}

pub async fn batch(State(s): State<AdminState>, Json(input): Json<BatchInput>) -> ApiResult {
    let grant = if input.operation == "grant_reset" {
        Some(codex2api_storage::ResetCardGrant {
            quantity: input.quantity.unwrap_or(0),
            note: input.note.unwrap_or_default().trim().into(),
            activate_at: input.activate_at,
            duration_days: input.duration_days.unwrap_or(30),
        })
    } else {
        if input.quantity.is_some()
            || input.note.is_some()
            || input.activate_at.is_some()
            || input.duration_days.is_some()
        {
            return Err(ApiError::bad("只有发卡操作可提交发卡参数"));
        }
        None
    };
    let selection = codex2api_storage::SubscriptionSelection {
        ids: input.ids,
        all_matching: input.all_matching,
        filters: input.filters,
        excluded_ids: input.excluded_ids,
    };
    let request_id = input
        .request_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    Ok(Json(
        s.storage
            .subscription_batch(&request_id, &input.operation, selection, grant)
            .await?,
    ))
}

pub async fn list(
    State(s): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    q.validate()?;
    let filters = codex2api_storage::SubscriptionFilters {
        user_id: q.user_id,
        provider_id: q.provider_id,
        plan_id: q.plan_id,
        search: q.search,
        status: q.status,
        subscription: q.subscription,
    };
    let (accounts, total, page) = s
        .storage
        .platform_account_page(&filters, q.page, q.page_size)
        .await?;
    let mut items = Vec::new();
    for account in accounts {
        let mut row = dto(&s, &account).await?;
        row["quota"] = s.storage.virtual_list_quota(&account).await?;
        items.push(row);
    }
    Ok(Json(
        json!({"items":items,"total":total,"page":page,"page_size":q.page_size.unwrap_or(20)}),
    ))
}

pub async fn options(
    State(s): State<AdminState>,
    Query(q): Query<crate::rest::dto::AccountListQuery>,
) -> ApiResult {
    let (search, limit) = q.search_params()?;
    let mut items = Vec::new();
    for account in s.storage.search_platform_accounts(search, limit).await? {
        items.push(dto(&s, &account).await?);
    }
    Ok(Json(json!({"items":items})))
}

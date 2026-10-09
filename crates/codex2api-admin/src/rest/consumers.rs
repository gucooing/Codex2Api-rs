use crate::{
    AdminState,
    rest::{
        error::{ApiError, ApiResult, ok},
        platform_accounts::{dto, require},
    },
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .merge(super::platform_accounts::router("/consumers"))
        .route("/consumers", get(list).post(create))
        .route("/consumers/options", get(options))
        .route("/consumers/batch", post(batch))
        .route(
            "/consumers/{id}",
            get(super::platform_accounts::detail)
                .put(update)
                .delete(delete),
        )
}

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
    filters: codex2api_storage::ConsumerFilters,
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
    let selection = codex2api_storage::ConsumerSelection {
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
            .consumer_batch(&request_id, &input.operation, selection, grant)
            .await?,
    ))
}

pub async fn list(
    State(s): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    q.validate()?;
    let filters = codex2api_storage::ConsumerFilters {
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    revision: Option<i64>,
    user_revision: Option<i64>,
    username: String,
    provider_id: String,
    #[serde(default)]
    password: String,
    name: String,
    email: String,
    plan_id: String,
    subscription_expires_at: Option<String>,
    enabled: bool,
}

pub async fn create(State(s): State<AdminState>, Json(input): Json<Input>) -> ApiResult {
    save(s, None, input).await
}

pub async fn update(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<Input>,
) -> ApiResult {
    save(s, Some(id), input).await
}

async fn save(s: AdminState, id: Option<String>, f: Input) -> ApiResult {
    let previous = match &id {
        Some(id) => Some(require(&s, id).await?),
        None => None,
    };
    let owner = match &id {
        Some(id) => Some(
            s.storage
                .virtual_user(id)
                .await?
                .ok_or_else(ApiError::missing)?,
        ),
        None => None,
    };
    if !codex2api_core::supported_provider(&f.provider_id)
        || previous
            .as_ref()
            .is_some_and(|a| a.provider_id != f.provider_id)
    {
        return Err(ApiError::bad("请选择支持的提供商，创建后不可切换"));
    }
    if f.username.trim().is_empty()
        || f.username.len() > 128
        || f.name.trim().is_empty()
        || f.name.len() > 128
        || f.email.len() > 254
        || !f.email.contains('@')
    {
        return Err(ApiError::bad("请填写有效的用户名、名称和邮箱"));
    }
    let plan = s
        .storage
        .virtual_plan(&f.plan_id)
        .await?
        .filter(|p| p.provider_id == f.provider_id)
        .ok_or_else(|| ApiError::bad("请选择可用的同提供商套餐"))?;
    let expiry = match f
        .subscription_expires_at
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        Some(v) => Some(
            chrono::DateTime::parse_from_rfc3339(v)
                .map_err(|_| ApiError::bad("订阅到期时间须为 RFC3339 格式"))?
                .to_rfc3339(),
        ),
        None => None,
    };
    let password_hash = if f.password.is_empty() {
        previous
            .as_ref()
            .ok_or_else(|| ApiError::bad("新账户必须设置密码"))?
            .password_hash
            .clone()
    } else {
        if f.password.len() > 1024 {
            return Err(ApiError::bad("密码过长"));
        }
        tokio::task::spawn_blocking(move || codex2api_storage::hash_password(&f.password))
            .await
            .map_err(|_| ApiError::bad("密码保存失败"))??
    };
    let user = codex2api_storage::User {
        kind: codex2api_storage::UserKind::Virtual,
        id: owner
            .as_ref()
            .map(|u| u.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        username: f.username,
        name: f.name,
        email: f.email,
        password_hash,
        enabled: f.enabled,
        wallet_cents: 0,
        revision: owner.as_ref().map_or(1, |u| u.revision),
        created_at: owner
            .as_ref()
            .map(|u| u.created_at.clone())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
    };
    let platform_id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    s.storage
        .save_virtual_user(codex2api_storage::VirtualUserChange {
            user: &user,
            platform_id: &platform_id,
            provider_id: &f.provider_id,
            plan_id: &plan.id,
            expires_at: expiry.as_deref(),
            user_revision: f.user_revision,
            revision: f.revision,
        })
        .await?;
    Ok(Json(dto(&s, &require(&s, &platform_id).await?).await?))
}

pub async fn delete(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    s.storage.delete_virtual_user(&id).await?;
    Ok(ok())
}

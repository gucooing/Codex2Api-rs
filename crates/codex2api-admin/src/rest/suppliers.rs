use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult, ok},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use codex2api_storage::{SupplierAccount, SupplierInfoSection, SupplierStatus};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .route(
            "/suppliers/grok/{id}/profile",
            post(crate::providers::grok::refresh_profile),
        )
        .nest(
            "/suppliers/chatgpt/oauth",
            crate::providers::chatgpt_oauth_routes(),
        )
        .nest(
            "/suppliers/grok/oauth",
            crate::providers::grok::oauth_routes(),
        )
        .route(
            "/suppliers/grok/{id}/models",
            get(crate::providers::grok::model_catalog)
                .post(crate::providers::grok::sync_model_catalog),
        )
        .route("/suppliers", get(list))
        .route("/suppliers/options", get(options))
        .route("/suppliers/selection", get(selection))
        .route("/suppliers/tags", post(super::supplier_tags::batch))
        .route(
            "/suppliers/oauth/setup",
            get(crate::providers::chatgpt::setup),
        )
        .route(
            "/suppliers/oauth/start",
            post(crate::providers::chatgpt::start),
        )
        .route(
            "/suppliers/oauth/callback",
            post(crate::providers::chatgpt::callback),
        )
        .route(
            "/suppliers/oauth/poll",
            post(crate::providers::chatgpt::poll),
        )
        .route(
            "/suppliers/oauth/cancel",
            post(crate::providers::chatgpt::cancel),
        )
        .route("/suppliers/{id}", get(detail).delete(delete))
        .route("/suppliers/{id}/status", post(status))
        .route("/suppliers/{id}/reset-state", post(reset_state))
        .route("/suppliers/{id}/quota", get(quota))
        .route(
            "/suppliers/{id}/fingerprint",
            put(crate::providers::fingerprint),
        )
        .route("/suppliers/{id}/official", get(crate::providers::official))
        .route(
            "/suppliers/chatgpt/{id}/official/rows",
            get(crate::providers::chatgpt::official_rows),
        )
        .route(
            "/suppliers/{id}/credits/consume",
            post(crate::providers::credit),
        )
        .route("/suppliers/{id}/relogin", post(crate::providers::relogin))
}

pub(crate) fn dto(a: &SupplierAccount) -> Value {
    json!({"id":a.id,"provider_id":a.provider_id,"proxy_id":a.proxy_id,"status":a.status,"display_name":a.display_name,"chatgpt_account_id":a.chatgpt_account_id,"chatgpt_user_id":a.chatgpt_user_id,"email":a.email,"plan_type":a.plan_type,"installation_id":a.installation_id,"originator":a.originator,"user_agent":a.user_agent,"os_type":a.os_type,"os_version":a.os_version,"arch":a.arch,"created_at":a.created_at,"updated_at":a.updated_at,"last_used_at":a.last_used_at})
}
fn cached_username(value: &Value) -> Option<String> {
    value
        .get("profile")
        .and_then(Value::as_object)
        .and_then(|profile| profile.get("username"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty())
}
pub(crate) async fn display(s: &AdminState, a: &SupplierAccount) -> Result<Value, ApiError> {
    let mut value = dto(a);
    value["subscription_expires_at"] =
        json!(crate::providers::subscription_expiration(s, a).await?);
    value["username"] = json!(
        s.storage
            .get_supplier_info(&a.id, SupplierInfoSection::Usage)
            .await?
            .and_then(|snapshot| cached_username(&snapshot.value))
    );
    let health = s.storage.supplier_health(&a.id).await?;
    let enabled = a.status == SupplierStatus::Active;
    let authorized = s
        .storage
        .load_supplier_tokens(&a.id)
        .await?
        .and_then(|t| t.access_token)
        .is_some_and(|t| !t.is_empty());
    value["enabled"] = json!(enabled);
    value["status"] = json!(health.display_status(enabled, authorized));
    value["authorized"] = json!(authorized);
    value["error_message"] = json!(if health.authentication_invalid {
        health.error_message.as_deref()
    } else if health.payment_required {
        Some("HTTP 402 Payment Required：供应账户付费/订阅不可用")
    } else if !authorized {
        Some("供应账户尚未完成授权")
    } else {
        None
    });
    value["error_at"] = json!(if health.authentication_invalid {
        &health.error_at
    } else {
        &health.payment_required_at
    });
    value["payment_required"] = json!(health.payment_required);
    value["payment_required_code"] = json!(health.payment_required_code);
    value["authentication_invalid"] = json!(health.authentication_invalid);
    value["cooldown_until"] = json!(health.cooldown_until);
    value["cooldown_code"] = json!(health.cooldown_code);
    let tags = s.storage.supplier_tag_labels(&a.id).await?;
    value["tag_ids"] = json!(tags.iter().map(|(id, _)| id).collect::<Vec<_>>());
    value["tags"] = json!(
        tags.into_iter()
            .map(|(id, name)| json!({"id":id,"name":name}))
            .collect::<Vec<_>>()
    );
    value["binding_count"] = json!(s.storage.supplier_binding_count(&a.id).await?);
    value["quota"] = match s.storage.get_account_quota(&a.id).await? {
        Some(snapshot) => crate::quota::summary(&s.storage, &a.id, &snapshot).await?,
        None => Value::Null,
    };
    Ok(value)
}
pub async fn list(
    State(s): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    let page = s.storage.supplier_page(&q).await?;
    let mut items = Vec::with_capacity(page.items.len());
    for id in page.items {
        items.push(display(&s, &s.storage.require_account(&id).await?).await?);
    }
    Ok(Json(
        json!({"items":items,"total":page.total,"page":page.page,"page_size":page.page_size}),
    ))
}
pub async fn selection(
    State(s): State<AdminState>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    Ok(Json(
        json!({"items":s.storage.supplier_selection(&q).await?}),
    ))
}
pub async fn options(
    State(s): State<AdminState>,
    Query(q): Query<crate::rest::dto::AccountListQuery>,
) -> ApiResult {
    let (search, limit) = q.search_params()?;
    let items = s
        .storage
        .supplier_options(
            search,
            limit,
            q.provider_id.as_deref(),
            q.tag.as_deref(),
            q.for_routing,
        )
        .await?;
    Ok(Json(json!({"items":items})))
}
pub async fn detail(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    let a = s.storage.require_account(&id).await?;
    let mut v = display(&s, &a).await?;
    v["fingerprint"] = crate::providers::fingerprint_view(&a);
    v["usage"] = json!(s.storage.account_usage_summary(&id).await?);
    Ok(Json(v))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusInput {
    enabled: bool,
}
pub async fn status(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<StatusInput>,
) -> ApiResult {
    s.storage.require_account(&id).await?;
    s.storage
        .set_account_status(
            &id,
            if f.enabled {
                SupplierStatus::Active
            } else {
                SupplierStatus::Disabled
            },
        )
        .await?;
    s.upstream.evict(&id).await;
    Ok(ok())
}
pub async fn reset_state(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    s.storage.require_account(&id).await?;
    s.storage.reset_supplier_availability(&id).await?;
    Ok(Json(
        display(&s, &s.storage.require_account(&id).await?).await?,
    ))
}
#[derive(Deserialize)]
pub struct QuotaQuery {
    #[serde(default)]
    refresh: bool,
}
pub async fn quota(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<QuotaQuery>,
) -> ApiResult {
    let account = s.storage.require_account(&id).await?;
    if account.status == SupplierStatus::Active
        && s.storage.supplier_health(&id).await?.authentication_invalid == false
    {
        crate::providers::quota(&s, &id, q.refresh)
            .await
            .map_err(ApiError::upstream)?;
    }
    Ok(Json(
        display(&s, &s.storage.require_account(&id).await?).await?,
    ))
}
pub async fn delete(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    if !s.storage.delete_account(&id).await? {
        return Err(ApiError::missing());
    }
    s.upstream.evict(&id).await;
    s.auth.evict_account_http(&id).await;
    s.supplier_cache.evict(&id).await;
    Ok(ok())
}

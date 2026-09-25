use super::error::{ApiError, ApiResult, ok};
use crate::AdminState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use codex2api_storage::{VirtualAccount, virtual_config_specs};
use serde::Deserialize;
use serde_json::{Value, json};
async fn dto(s: &AdminState, a: &VirtualAccount) -> Result<Value, ApiError> {
    let mut value = super::dto::Consumer::from(a);
    value.plan_name = s
        .storage
        .virtual_plan(&a.plan_id)
        .await?
        .map(|p| p.name)
        .unwrap_or_else(|| "已删除套餐".into());
    Ok(super::dto::value(value))
}
async fn require(state: &AdminState, id: &str) -> Result<VirtualAccount, ApiError> {
    state
        .storage
        .virtual_account(id)
        .await?
        .ok_or_else(ApiError::missing)
}
pub async fn list(
    State(s): State<AdminState>,
    Query(q): Query<super::dto::AccountListQuery>,
) -> ApiResult {
    let accounts = match q.search_params()? {
        Some((search, limit)) => s.storage.search_virtual_accounts(search, limit).await?,
        None => s.storage.virtual_accounts().await?,
    };
    let mut items = Vec::new();
    for account in accounts {
        items.push(dto(&s, &account).await?);
    }
    Ok(Json(json!({"items":items})))
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct BatchFilters {
    search: String,
    status: String,
    subscription: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchInput {
    operation: String,
    #[serde(default)]
    ids: Vec<String>,
    #[serde(default)]
    all_matching: bool,
    #[serde(default)]
    filters: BatchFilters,
    #[serde(default)]
    quantity: i64,
    #[serde(default)]
    note: String,
    #[serde(default)]
    activate_at: Option<String>,
    #[serde(default = "default_reset_duration_days")]
    duration_days: i64,
}
pub async fn batch(State(s): State<AdminState>, Json(input): Json<BatchInput>) -> ApiResult {
    if !input.all_matching && input.ids.is_empty() {
        return Err(ApiError::bad("请选择虚拟账户或当前筛选结果"));
    }
    let ids = if input.all_matching {
        let enabled = match input.filters.status.as_str() {
            "" => None,
            "enabled" => Some(true),
            "disabled" => Some(false),
            _ => return Err(ApiError::bad("登录状态筛选无效")),
        };
        s.storage
            .virtual_account_ids_matching(
                &input.filters.search,
                enabled,
                Some(&input.filters.subscription),
            )
            .await?
    } else {
        input.ids
    };
    if ids.is_empty() {
        return Ok(Json(json!({"ok":true,"affected":0})));
    }
    match input.operation.as_str() {
        "delete" => {
            for id in &ids {
                s.storage.delete_virtual_account(id).await?;
            }
        }
        "reset" => {
            for id in &ids {
                let value = s.storage.admin_reset_virtual_quota(id, "admin").await?;
                if value["code"] == "nothing_to_reset" {
                    continue;
                }
            }
        }
        "grant_reset" => {
            if !(1..=100).contains(&input.quantity) {
                return Err(ApiError::bad("每个账户发放数量须为 1 至 100"));
            }
            let available_at = input
                .activate_at
                .as_deref()
                .filter(|v| !v.trim().is_empty())
                .map(|v| {
                    chrono::DateTime::parse_from_rfc3339(v)
                        .map(|d| d.timestamp_millis())
                        .map_err(|_| ApiError::bad("启用时间须为 RFC3339 格式"))
                })
                .transpose()?
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
            if !(1..=3650).contains(&input.duration_days) {
                return Err(ApiError::bad("有效时长须为 1 至 3650 天"));
            }
            let expires = available_at
                .checked_add(input.duration_days.saturating_mul(86_400_000))
                .ok_or_else(|| ApiError::bad("有效时长无效"))?;
            for id in &ids {
                s.storage
                    .grant_virtual_reset_credits_scheduled(
                        id,
                        &format!("batch-grant-{}-{}", uuid::Uuid::new_v4(), id),
                        input.quantity,
                        input.note.trim(),
                        available_at,
                        Some(expires),
                    )
                    .await?;
            }
        }
        _ => return Err(ApiError::bad("批量操作类型无效")),
    }
    Ok(Json(json!({"ok":true,"affected":ids.len()})))
}
pub async fn detail(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    Ok(Json(dto(&s, &require(&s, &id).await?).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
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
    if !codex2api_core::supported_provider(&f.provider_id)
        || previous
            .as_ref()
            .is_some_and(|a| a.provider_id != f.provider_id)
    {
        return Err(ApiError::bad("提供商必须在创建时确定，当前仅支持 ChatGPT"));
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
        .filter(|p| {
            p.provider_id == f.provider_id
                && (p.enabled || previous.as_ref().is_some_and(|a| a.plan_id == p.id))
        })
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
    let account = VirtualAccount {
        id: id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        provider_id: f.provider_id,
        username: f.username.trim().into(),
        name: f.name.trim().into(),
        email: f.email.trim().into(),
        password_hash,
        plan_id: plan.id,
        plan_type: plan.plan_type,
        subscription_expires_at: expiry,
        enabled: f.enabled,
        created_at: previous
            .map(|a| a.created_at)
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
    };
    s.storage
        .save_virtual_account_operation(&account, "admin")
        .await?;
    Ok(Json(dto(&s, &account).await?))
}
pub async fn delete(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    s.storage.delete_virtual_account(&id).await?;
    Ok(ok())
}
pub async fn usage(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(
        json!({"summary":s.storage.virtual_usage_summary(&id).await?,"quota":s.storage.virtual_quota(&id).await?}),
    ))
}
pub async fn reset_credits(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(s.storage.virtual_reset_credit_records(&id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetGrant {
    request_id: String,
    quantity: i64,
    #[serde(default)]
    note: String,
    #[serde(default)]
    activate_at: Option<String>,
    #[serde(default = "default_reset_duration_days")]
    duration_days: i64,
}
fn default_reset_duration_days() -> i64 {
    30
}
pub async fn grant_reset_credits(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<ResetGrant>,
) -> ApiResult {
    require(&s, &id).await?;
    let available_at = input
        .activate_at
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .map(|v| {
            chrono::DateTime::parse_from_rfc3339(v)
                .map(|d| d.timestamp_millis())
                .map_err(|_| ApiError::bad("启用时间须为 RFC3339 格式"))
        })
        .transpose()?
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    if !(1..=3650).contains(&input.duration_days) {
        return Err(ApiError::bad("有效时长须为 1 至 3650 天"));
    }
    let expires_at = available_at
        .checked_add(input.duration_days.saturating_mul(86_400_000))
        .ok_or_else(|| ApiError::bad("有效时长无效"))?;
    s.storage
        .grant_virtual_reset_credits_scheduled(
            &id,
            &input.request_id,
            input.quantity,
            input.note.trim(),
            available_at,
            Some(expires_at),
        )
        .await?;
    Ok(Json(s.storage.virtual_reset_credit_records(&id).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetConsume {
    redeem_request_id: String,
    credit_id: String,
}
pub async fn consume_reset_credit(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<ResetConsume>,
) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(
        s.storage
            .consume_virtual_reset_credit(
                &id,
                &input.redeem_request_id,
                Some(&input.credit_id),
                "admin",
            )
            .await?,
    ))
}
pub async fn devices(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(
        json!({"items":s.storage.virtual_devices(&id).await?,"remote_servers":s.storage.remote_servers(&id).await?}),
    ))
}
pub async fn revoke(
    State(s): State<AdminState>,
    Path((id, device)): Path<(String, String)>,
) -> ApiResult {
    require(&s, &id).await?;
    if !s
        .storage
        .virtual_devices(&id)
        .await?
        .iter()
        .any(|d| d.id == device)
    {
        return Err(ApiError::missing());
    }
    s.storage.revoke_virtual_device(&id, &device).await?;
    Ok(ok())
}
#[derive(Deserialize)]
#[serde(default)]
pub struct LogQuery {
    page: u32,
    page_size: Option<u32>,
    path: String,
    method: String,
    result: String,
}
impl Default for LogQuery {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: None,
            path: String::new(),
            method: String::new(),
            result: String::new(),
        }
    }
}
pub async fn logs(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<LogQuery>,
) -> ApiResult {
    require(&s, &id).await?;
    codex2api_storage::table_page_size(q.page_size).map_err(|e| ApiError::bad(e.to_string()))?;
    if !matches!(q.result.as_str(), "" | "success" | "failed") {
        return Err(ApiError::bad("日志状态无效"));
    }
    let mut value = s
        .storage
        .virtual_request_log_page(&id, q.page, q.page_size, &q.path, &q.method, &q.result)
        .await?;
    value["analytics"] = json!(s.storage.virtual_analytics(&id, 0, i64::MAX).await?);
    value["site_status"] = json!(s.storage.virtual_resources(&id, "site_status").await?);
    Ok(Json(value))
}
#[derive(Deserialize)]
pub struct RecordQuery {
    #[serde(default = "task")]
    kind: String,
}
fn task() -> String {
    "task".into()
}
pub async fn records(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<RecordQuery>,
) -> ApiResult {
    require(&s, &id).await?;
    if q.kind == "family_notices" {
        return Ok(Json(
            json!({"items":s.storage.family_graduation_notices(&id,true).await?}),
        ));
    }
    if !matches!(
        q.kind.as_str(),
        "task"
            | "realtime_call"
            | "conversation"
            | "connector_catalog"
            | "task_turn"
            | "task_read"
            | "task_execution"
            | "task_operation"
            | "mcp_operation"
            | "plugin_operation"
            | "automation_operation"
            | "subscription_operation"
    ) {
        return Err(ApiError::bad("未知记录类别"));
    }
    Ok(Json(
        json!({"items":s.storage.virtual_resource_records(&id,&q.kind).await?}),
    ))
}
pub async fn client_state(
    State(s): State<AdminState>,
    Path((id, key)): Path<(String, String)>,
) -> ApiResult {
    require(&s, &id).await?;
    if !virtual_config_specs().iter().any(|v| v.key == key) {
        return Err(ApiError::missing());
    }
    let value = s.storage.virtual_client_state(&id, &key).await?;
    let mut response = match value {
        Some(v) => {
            json!({"key":key,"value":v.value,"revision":v.revision,"write_origin":v.write_origin,"updated_at_ms":v.updated_at_ms})
        }
        None => {
            json!({"key":key,"value":null,"revision":null,"write_origin":null,"updated_at_ms":null})
        }
    };
    response["fields"] = json!(codex2api_storage::client_fields(&key));
    Ok(Json(response))
}
pub async fn configs(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    let mut items = Vec::new();
    for spec in virtual_config_specs() {
        if codex2api_storage::plan_owned_config(spec.key) {
            continue;
        }
        let readonly = spec.key == "models" || codex2api_storage::client_state_only(spec.key);
        let stored = if readonly {
            s.storage.virtual_client_state(&id, spec.key).await?
        } else {
            Some(s.storage.virtual_config(&id, spec.key).await?)
        };
        let (value, revision, origin, updated) = stored
            .map(|v| (v.value, v.revision, v.write_origin, v.updated_at_ms))
            .unwrap_or((Value::Null, 0, "unknown".into(), 0));
        items.push(json!({"key":spec.key,"label":spec.label,"description":spec.description,"readonly":readonly,"value":if readonly{value}else{codex2api_storage::editable_virtual_contract(spec.key,&value)},"revision":revision,"write_origin":origin,"updated_at_ms":updated,"fields":if readonly{codex2api_storage::client_fields(spec.key)}else{codex2api_storage::admin_client_fields(spec.key)}}));
    }
    Ok(Json(json!({"items":items})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigInput {
    revision: i64,
    value: Value,
}
pub async fn save_config(
    State(s): State<AdminState>,
    Path((id, key)): Path<(String, String)>,
    Json(mut f): Json<ConfigInput>,
) -> ApiResult {
    require(&s, &id).await?;
    if key == "models"
        || codex2api_storage::plan_owned_config(&key)
        || codex2api_storage::client_state_only(&key)
    {
        return Err(ApiError::forbidden(
            "该数据不由账户配置编辑：客户端记录只读，套餐权益在套餐管理维护",
        ));
    }
    if !virtual_config_specs().iter().any(|v| v.key == key) {
        return Err(ApiError::missing());
    }
    codex2api_storage::prepare_client_config(&key, &mut f.value);
    codex2api_storage::prepare_virtual_contract(&key, &mut f.value);
    if key == "workspace_messages" {
        for item in f.value["messages"].as_array_mut().into_iter().flatten() {
            if item.is_object() && item["message_id"].as_str().is_none_or(|v| v.is_empty()) {
                item["message_id"] = uuid::Uuid::new_v4().to_string().into();
            }
        }
    }
    let previous = s.storage.virtual_config(&id, &key).await?;
    if previous.revision != f.revision {
        return Err(ApiError::conflict());
    }
    codex2api_storage::validate_admin_client_change(&key, &previous.value, &f.value)
        .map_err(ApiError::forbidden)?;
    codex2api_storage::validate_virtual_config(&key, &f.value).map_err(ApiError::bad)?;
    let saved = s
        .storage
        .update_virtual_admin_config(&id, &key, &f.value, f.revision)
        .await?
        .ok_or_else(ApiError::conflict)?;
    Ok(Json(
        json!({"revision":saved,"value":codex2api_storage::editable_virtual_contract(&key,&f.value)}),
    ))
}
pub async fn routes(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    let routes = s
        .storage
        .execution_routes(&id)
        .await?
        .into_iter()
        .map(|r| super::dto::Route {
            virtual_account_id: r.virtual_account_id,
            provider_id: r.provider_id,
            supplier_account_id: r.supplier_account_id,
            revision: r.revision,
        })
        .collect::<Vec<_>>();
    Ok(Json(super::dto::value(super::dto::Routes {
        items: routes,
    })))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteInput {
    supplier_id: Option<String>,
    revision: Option<i64>,
}
pub async fn save_route(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<RouteInput>,
) -> ApiResult {
    let account = require(&s, &id).await?;
    if !s
        .storage
        .save_execution_route(
            &id,
            &account.provider_id,
            f.supplier_id.as_deref().filter(|s| !s.is_empty()),
            f.revision,
        )
        .await?
    {
        return Err(ApiError::conflict());
    }
    routes(State(s), Path(id)).await
}

pub async fn plugins(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    let items = s
        .storage
        .virtual_client_state(&id, "installed_plugins")
        .await?
        .and_then(|v| v.value["plugins"].as_array().cloned())
        .unwrap_or_default();
    Ok(Json(json!({"items": items})))
}
pub async fn connectors(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(
        json!({"items": s.storage.virtual_resources(&id, "connector_catalog").await?}),
    ))
}

use super::error::{ApiError, ApiResult, ok};
use crate::AdminState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use codex2api_storage::{PlatformAccount, virtual_config_specs};
use serde::Deserialize;
use serde_json::{Value, json};
async fn dto(s: &AdminState, a: &PlatformAccount) -> Result<Value, ApiError> {
    let mut value = super::dto::Consumer::from(a);
    let user = s
        .storage
        .platform_account_user(&a.id)
        .await?
        .ok_or_else(ApiError::missing)?;
    value.user_id = Some(user.id);
    value.user_kind = user.kind;
    value.user_revision = user.revision;
    value.revision = s.storage.platform_revision(&a.id).await?;
    value.plan_name = s
        .storage
        .virtual_plan(&a.plan_id)
        .await?
        .map(|p| p.name)
        .unwrap_or_else(|| "已删除套餐".into());
    Ok(super::dto::value(value))
}
async fn require(state: &AdminState, id: &str) -> Result<PlatformAccount, ApiError> {
    state
        .storage
        .platform_account(id)
        .await?
        .ok_or_else(ApiError::missing)
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
    Query(q): Query<super::dto::AccountListQuery>,
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
pub async fn detail(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    Ok(Json(dto(&s, &require(&s, &id).await?).await?))
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
pub async fn usage(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(
        json!({"summary":s.storage.virtual_usage_summary(&id).await?,"quota":s.storage.virtual_quota(&id).await?}),
    ))
}
pub async fn reset_credits(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(s.storage.virtual_reset_credit_records(&id, &q).await?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetGrant {
    request_id: Option<String>,
    quantity: i64,
    #[serde(default)]
    note: String,
    #[serde(default)]
    activate_at: Option<chrono::DateTime<chrono::Utc>>,
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
    let grant = codex2api_storage::ResetCardGrant {
        quantity: input.quantity,
        note: input.note.trim().into(),
        activate_at: input.activate_at,
        duration_days: input.duration_days,
    };
    let request_id = input
        .request_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    s.storage
        .grant_virtual_reset_cards(&id, &request_id, &grant)
        .await?;
    Ok(Json(
        s.storage
            .virtual_reset_credit_records(&id, &codex2api_storage::ListQuery::default())
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetConsume {
    redeem_request_id: Option<String>,
    credit_id: String,
}
pub async fn consume_reset_credit(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<ResetConsume>,
) -> ApiResult {
    require(&s, &id).await?;
    let redeem_request_id = input
        .redeem_request_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    Ok(Json(
        s.storage
            .consume_virtual_reset_credit(&id, &redeem_request_id, Some(&input.credit_id), "admin")
            .await?,
    ))
}
pub async fn devices(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(json!(s.storage.device_page(&id, &q).await?)))
}
pub async fn remote_servers(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(json!(s.storage.remote_server_page(&id, &q).await?)))
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
    let value = s
        .storage
        .virtual_request_log_page(&id, q.page, q.page_size, &q.path, &q.method, &q.result)
        .await?;
    Ok(Json(value))
}
pub async fn records(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    require(&s, &id).await?;
    if q.kind == "cloud_environment" {
        return Ok(Json(json!(
            s.storage.cloud_environment_page(&id, &q).await?
        )));
    }
    if !matches!(
        q.kind.as_str(),
        "" | "task"
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
            | "family_notices"
            | "analytics"
            | "site_status"
    ) {
        return Err(ApiError::bad("未知记录类别"));
    }
    Ok(Json(json!(s.storage.resource_record_page(&id, &q).await?)))
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
            let mut value = v.value;
            if key == "browser_settings" {
                value["rules"] = json!({});
            }
            client_metadata(&mut value);
            json!({"key":key,"value":value,"revision":v.revision,"write_origin":v.write_origin,"updated_at_ms":v.updated_at_ms})
        }
        None => {
            json!({"key":key,"value":null,"revision":null,"write_origin":null,"updated_at_ms":null})
        }
    };
    response["fields"] = json!(codex2api_storage::client_fields(&key));
    Ok(Json(response))
}
fn client_metadata(value: &mut Value) {
    match value {
        Value::Array(items) => items.clear(),
        Value::Object(fields) => fields.values_mut().for_each(client_metadata),
        _ => {}
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientRowsQuery {
    #[serde(default = "first_page")]
    page: u32,
    page_size: Option<u32>,
    section: String,
    #[serde(default)]
    entries: bool,
}
fn first_page() -> u32 {
    1
}
pub async fn client_rows(
    State(s): State<AdminState>,
    Path((id, key)): Path<(String, String)>,
    Query(q): Query<ClientRowsQuery>,
) -> ApiResult {
    require(&s, &id).await?;
    if !virtual_config_specs().iter().any(|v| v.key == key) {
        return Err(ApiError::missing());
    }
    Ok(Json(json!(
        s.storage
            .client_state_rows(
                &id,
                &key,
                &q.section,
                q.entries,
                &codex2api_storage::ListQuery {
                    page: q.page,
                    page_size: q.page_size,
                    ..Default::default()
                }
            )
            .await?
    )))
}
pub async fn configs(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    let account = require(&s, &id).await?;
    if account.provider_id != codex2api_core::CHATGPT {
        // This channel's service settings are its identity, plan and execution route.
        // Do not materialize another channel's Desktop configuration defaults.
        return Ok(Json(json!({"items":[]})));
    }
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
    if require(&s, &id).await?.provider_id != codex2api_core::CHATGPT {
        return Err(ApiError::bad("该渠道的配置在账户、套餐和供应路由中维护"));
    }
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
            tag_id: r.tag_id,
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
    tag_id: Option<String>,
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
        .save_pool_route(
            &id,
            &account.provider_id,
            f.tag_id.as_deref().filter(|s| !s.is_empty()),
            f.supplier_id.as_deref().filter(|s| !s.is_empty()),
            f.revision,
        )
        .await?
    {
        return Err(ApiError::conflict());
    }
    routes(State(s), Path(id)).await
}

pub async fn rate_limit(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(json!(s.storage.virtual_rpm_limit(&id).await?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpmInput {
    rpm: Option<u32>,
}

pub async fn save_rate_limit(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(input): Json<RpmInput>,
) -> ApiResult {
    require(&s, &id).await?;
    s.storage.save_virtual_rpm_limit(&id, input.rpm).await?;
    rate_limit(State(s), Path(id)).await
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

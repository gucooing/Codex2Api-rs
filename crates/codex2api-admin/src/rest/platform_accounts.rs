use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult, ok},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use codex2api_storage::{PlatformAccount, virtual_config_specs};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn router(prefix: &str) -> Router<AdminState> {
    Router::new()
        .route(
            &format!("{prefix}/{{id}}/rate-limit"),
            get(rate_limit).put(save_rate_limit),
        )
        .route(&format!("{prefix}/{{id}}/usage"), get(usage))
        .route(
            &format!("{prefix}/{{id}}/reset-credits"),
            get(reset_credits).post(grant_reset_credits),
        )
        .route(
            &format!("{prefix}/{{id}}/reset-credits/consume"),
            post(consume_reset_credit),
        )
        .route(&format!("{prefix}/{{id}}/configs"), get(configs))
        .route(&format!("{prefix}/{{id}}/plugins"), get(plugins))
        .route(&format!("{prefix}/{{id}}/connectors"), get(connectors))
        .route(&format!("{prefix}/{{id}}/config/{{key}}"), put(save_config))
        .route(
            &format!("{prefix}/{{id}}/client-state/{{key}}"),
            get(client_state),
        )
        .route(
            &format!("{prefix}/{{id}}/client-state/{{key}}/rows"),
            get(client_rows),
        )
        .route(&format!("{prefix}/{{id}}/devices"), get(devices))
        .route(
            &format!("{prefix}/{{id}}/remote-servers"),
            get(remote_servers),
        )
        .route(
            &format!("{prefix}/{{id}}/devices/{{device}}/revoke"),
            post(revoke),
        )
        .route(
            &format!("{prefix}/{{id}}/routing"),
            get(routes).put(save_route),
        )
        .route(&format!("{prefix}/{{id}}/logs"), get(logs))
        .route(&format!("{prefix}/{{id}}/records"), get(records))
}

pub(crate) async fn dto(s: &AdminState, a: &PlatformAccount) -> Result<Value, ApiError> {
    let mut value = crate::rest::dto::Consumer::from(a);
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
    Ok(crate::rest::dto::value(value))
}

pub(crate) async fn require(state: &AdminState, id: &str) -> Result<PlatformAccount, ApiError> {
    state
        .storage
        .platform_account(id)
        .await?
        .ok_or_else(ApiError::missing)
}

pub async fn detail(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    Ok(Json(dto(&s, &require(&s, &id).await?).await?))
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

pub async fn routes(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    let routes = s
        .storage
        .execution_routes(&id)
        .await?
        .into_iter()
        .map(|r| crate::rest::dto::Route {
            tag_id: r.tag_id,
            virtual_account_id: r.virtual_account_id,
            provider_id: r.provider_id,
            supplier_account_id: r.supplier_account_id,
            revision: r.revision,
        })
        .collect::<Vec<_>>();
    Ok(Json(crate::rest::dto::value(crate::rest::dto::Routes {
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

pub async fn usage(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    require(&s, &id).await?;
    Ok(Json(
        json!({"summary":s.storage.virtual_usage_summary(&id).await?,"quota":s.storage.virtual_quota(&id).await?}),
    ))
}

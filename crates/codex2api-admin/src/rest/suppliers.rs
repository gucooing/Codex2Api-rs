use super::error::{ApiError, ApiResult, ok};
use crate::AdminState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use codex2api_accounts::{AccountIdentity, HostRuntime, new_installation_id};
use codex2api_auth::{CompletedLogin, LoginFlow};
use codex2api_storage::{SupplierAccount, SupplierInfoSection, SupplierStatus};
use codex2api_upstream::BackendEndpoint as E;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
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
    value["username"] = json!(
        s.storage
            .get_supplier_info(&a.id, SupplierInfoSection::Usage)
            .await?
            .and_then(|snapshot| cached_username(&snapshot.value))
    );
    let health = s.storage.supplier_health(&a.id).await?;
    value["status"] = json!(if a.status != SupplierStatus::Active {
        "disabled"
    } else if health.error_message.is_some() {
        "error"
    } else {
        "active"
    });
    value["authorized"] = json!(
        a.status != SupplierStatus::Pending
            && s.storage
                .load_supplier_tokens(&a.id)
                .await?
                .and_then(|t| t.access_token)
                .is_some_and(|t| !t.is_empty())
    );
    value["error_message"] = json!(health.error_message);
    value["error_at"] = json!(health.error_at);
    value["quota"] = match s.storage.get_account_quota(&a.id).await? {
        Some(snapshot) => crate::quota::summary(&snapshot),
        None => Value::Null,
    };
    Ok(value)
}
pub async fn list(
    State(s): State<AdminState>,
    Query(q): Query<super::dto::AccountListQuery>,
) -> ApiResult {
    let accounts = match q.search_params()? {
        Some((search, limit)) => {
            s.storage
                .search_supplier_accounts(search, limit, q.provider_id.as_deref(), q.for_routing)
                .await?
        }
        None => s.storage.list_accounts().await?,
    };
    let mut items = vec![];
    for a in accounts {
        let mut value = display(&s, &a).await?;
        let expires = s
            .storage
            .load_supplier_tokens(&a.id)
            .await?
            .and_then(|t| t.id_token)
            .and_then(|t| {
                codex2api_auth::parse_chatgpt_subscription_expiration(&t)
                    .ok()
                    .flatten()
            });
        value["subscription_expires_at"] = json!(expires);
        items.push(value);
    }
    Ok(Json(json!({"items":items})))
}
pub async fn detail(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    let a = s.storage.require_account(&id).await?;
    let mut v = display(&s, &a).await?;
    v["fingerprint"] = json!(Fingerprint::from_account(&a));
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
    let a = s.storage.require_account(&id).await?;
    if f.enabled
        && (a.status == SupplierStatus::Pending
            || s.storage
                .load_supplier_tokens(&id)
                .await?
                .and_then(|t| t.access_token)
                .is_none_or(|t| t.is_empty()))
    {
        return Err(ApiError::bad("供应账户需先完成授权"));
    }
    if f.enabled
        && s.storage
            .supplier_health(&id)
            .await?
            .error_message
            .is_some()
    {
        return Err(ApiError::bad("请先通过更多菜单恢复官方通信，再启用账户"));
    }
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
pub async fn recover(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    let account = s.storage.require_account(&id).await?;
    if account.status == SupplierStatus::Pending {
        return Err(ApiError::bad("供应账户需先完成授权"));
    }
    let health = s.storage.supplier_health(&id).await?;
    // A real request is required; a healthy cached snapshot cannot prove recovery.
    crate::services::quota(&s, &id, true)
        .await
        .map_err(ApiError::upstream)?;
    if health.error_message.is_some() && !s.storage.recover_supplier(&id, health.revision).await? {
        return Err(ApiError::bad("检查期间发生了新的通信错误，请重新恢复"));
    }
    s.upstream.evict(&id).await;
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
        && s.storage
            .supplier_health(&id)
            .await?
            .error_message
            .is_none()
    {
        crate::services::quota(&s, &id, q.refresh)
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
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fingerprint {
    os_type: String,
    os_version: String,
    arch: String,
    terminal: String,
    proxy_id: Option<String>,
    #[serde(default)]
    timezone: String,
}
impl Fingerprint {
    fn from_identity(a: &AccountIdentity) -> Self {
        Self {
            os_type: a.os_type.clone(),
            os_version: a.os_version.clone(),
            arch: a.arch.clone(),
            terminal: a
                .official_user_agent()
                .split_once(") ")
                .map_or("unknown", |(_, s)| s)
                .into(),
            proxy_id: None,
            timezone: a.http_fingerprint.timezone.clone().unwrap_or_default(),
        }
    }
    fn from_account(a: &SupplierAccount) -> Self {
        let mut f = Self::from_identity(&AccountIdentity::from_account(a));
        f.proxy_id = a.proxy_id.clone();
        f
    }
    fn identity(&self, id: String, installation: String) -> Result<AccountIdentity, ApiError> {
        if !self.timezone.trim().is_empty() && self.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(ApiError::bad("请输入有效的 IANA 时区"));
        }
        for (label, value, max) in [
            ("操作系统", &self.os_type, 128),
            ("系统版本", &self.os_version, 128),
            ("架构", &self.arch, 128),
            ("终端标识", &self.terminal, 256),
        ] {
            if value.trim().is_empty()
                || value.len() > max
                || !value.bytes().all(|b| (b' '..=b'~').contains(&b))
                || (label != "终端标识" && value.contains(['(', ')', ';']))
            {
                return Err(ApiError::bad(format!("{label}格式无效")));
            }
        }
        let runtime = HostRuntime {
            originator: codex2api_version::DEFAULT_ORIGINATOR.into(),
            user_agent: codex2api_version::official_user_agent(
                self.os_type.trim(),
                self.os_version.trim(),
                self.arch.trim(),
                self.terminal.trim(),
            ),
            os_type: self.os_type.trim().into(),
            os_version: self.os_version.trim().into(),
            arch: self.arch.trim().into(),
        };
        let mut a = AccountIdentity::new(id, installation, runtime);
        a.http_fingerprint.timezone =
            (!self.timezone.trim().is_empty()).then(|| self.timezone.trim().into());
        Ok(a)
    }
    async fn validate_proxy(&self, s: &AdminState) -> Result<(), ApiError> {
        if let Some(id) = self.proxy_id.as_deref().filter(|v| !v.is_empty()) {
            s.storage.require_outbound_proxy(id).await?;
        }
        Ok(())
    }
}
pub async fn fingerprint(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<Fingerprint>,
) -> ApiResult {
    let mut a = s.storage.require_account(&id).await?;
    f.validate_proxy(&s).await?;
    let identity = f.identity(a.id.clone(), a.installation_id.clone())?;
    let mut fp = AccountIdentity::from_account(&a).http_fingerprint;
    fp.os_type = identity.os_type.clone();
    fp.os_version = identity.os_version.clone();
    fp.arch = identity.arch.clone();
    fp.user_agent = identity.official_user_agent();
    fp.originator = codex2api_version::DEFAULT_ORIGINATOR.into();
    fp.timezone = identity.http_fingerprint.timezone;
    a.os_type = fp.os_type.clone();
    a.os_version = fp.os_version.clone();
    a.arch = fp.arch.clone();
    a.originator = fp.originator.clone();
    a.user_agent = fp.user_agent.clone();
    a.http_fingerprint_json = fp.to_json().map_err(|_| ApiError::bad("指纹配置无效"))?;
    a.proxy_id = f.proxy_id.filter(|v| !v.is_empty());
    s.storage.save_account_fingerprint(&a).await?;
    s.upstream.evict(&id).await;
    s.auth
        .reload_account_http(&id)
        .await
        .map_err(|_| ApiError::upstream("指纹已保存，但 HTTP 客户端更新失败"))?;
    Ok(Json(json!(Fingerprint::from_account(&a))))
}
#[derive(Deserialize)]
pub struct OfficialQuery {
    section: String,
    #[serde(default)]
    refresh: bool,
}
pub async fn official(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<OfficialQuery>,
) -> ApiResult {
    s.storage.require_account(&id).await?;
    let (section, endpoint) = match q.section.as_str() {
        "quota" => (SupplierInfoSection::Quota, E::Usage),
        "usage" => (SupplierInfoSection::Usage, E::Profile),
        "details" => (SupplierInfoSection::Details, E::Accounts),
        "credits" => (SupplierInfoSection::Credits, E::Credits),
        _ => return Err(ApiError::bad("未知官方数据类别")),
    };
    let result = if section == SupplierInfoSection::Quota {
        crate::services::quota(&s, &id, q.refresh).await
    } else {
        s.supplier_cache
            .get_or_fetch(&id, section, q.refresh, || async {
                crate::services::request(&s, &id, endpoint, &HashMap::new(), None, None).await
            })
            .await
    };
    let (snapshot, refresh_error) = match result {
        Ok(snapshot) => (snapshot, None),
        Err(error) => match s.storage.get_supplier_info(&id, section).await? {
            Some(snapshot) => (snapshot, Some(error)),
            None => return Err(ApiError::upstream(error)),
        },
    };
    let mut value = json!({"value":snapshot.value,"observed_at":snapshot.observed_at,"refresh_error":refresh_error});
    if section == SupplierInfoSection::Quota {
        value["quota"] = crate::quota::summary(&snapshot);
    }
    Ok(Json(value))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditInput {
    #[serde(default)]
    credit_id: String,
    redeem_request_id: Option<String>,
}
fn credit_payload(f: CreditInput) -> Result<Value, ApiError> {
    let request_id = f
        .redeem_request_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    uuid::Uuid::parse_str(&request_id).map_err(|_| ApiError::bad("额度重置请求标识无效"))?;
    let mut body = json!({"redeem_request_id":request_id});
    if !f.credit_id.is_empty() {
        body["credit_id"] = f.credit_id.into();
    }
    Ok(body)
}
pub async fn credit(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<CreditInput>,
) -> ApiResult {
    let body = credit_payload(f)?;
    let value =
        crate::services::request(&s, &id, E::ConsumeCredit, &HashMap::new(), None, Some(body))
            .await
            .map_err(ApiError::upstream)?;
    s.supplier_cache
        .invalidate(&id, SupplierInfoSection::Quota)
        .await;
    s.supplier_cache
        .invalidate(&id, SupplierInfoSection::Credits)
        .await;
    Ok(Json(value))
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Callback,
    Device,
    RefreshToken,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartInput {
    method: Method,
    fingerprint: Fingerprint,
    #[serde(default)]
    refresh_token: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relogin {
    method: Method,
}
pub async fn setup() -> ApiResult {
    let identity = AccountIdentity::new(
        uuid::Uuid::new_v4().to_string(),
        new_installation_id(),
        HostRuntime::generate(),
    );
    Ok(Json(
        json!({"fingerprint":Fingerprint::from_identity(&identity)}),
    ))
}
fn pending(s: &AdminState, p: &codex2api_storage::OAuthPending) -> ApiResult {
    let flow = s
        .auth
        .login_flow(p)
        .map_err(|_| ApiError::bad("授权流程不可用"))?;
    Ok(Json(match flow {
        LoginFlow::Callback { authorize_url } => {
            json!({"status":"pending","method":"callback","state":p.state,"authorize_url":authorize_url})
        }
        LoginFlow::Device {
            verification_url,
            user_code,
            interval,
            ..
        } => {
            json!({"status":"pending","method":"device","state":p.state,"verification_url":verification_url,"user_code":user_code,"interval":interval})
        }
    }))
}
async fn completed(s: &AdminState, done: CompletedLogin) -> ApiResult {
    s.supplier_cache.invalidate_all(&done.account.id).await;
    match crate::services::request(s, &done.account.id, E::Profile, &HashMap::new(), None, None)
        .await
    {
        Ok(value) => {
            let snapshot = codex2api_storage::QuotaSnapshot {
                value,
                observed_at: chrono::Utc::now(),
            };
            if let Err(error) = s
                .storage
                .store_supplier_info(&done.account.id, SupplierInfoSection::Usage, &snapshot)
                .await
            {
                tracing::warn!(%error, account_id = %done.account.id, "failed to cache authorized supplier profile");
            }
        }
        Err(error) => {
            tracing::warn!(%error, account_id = %done.account.id, "failed to load authorized supplier profile");
        }
    }
    s.upstream.evict(&done.account.id).await;
    Ok(Json(
        json!({"status":"complete","supplier_id":done.account.id}),
    ))
}
pub async fn start(State(s): State<AdminState>, Json(f): Json<StartInput>) -> ApiResult {
    f.fingerprint.validate_proxy(&s).await?;
    let identity = f
        .fingerprint
        .identity(uuid::Uuid::new_v4().to_string(), new_installation_id())?;
    if matches!(f.method, Method::RefreshToken) {
        let done = s
            .auth
            .login_with_refresh_token(
                identity,
                f.fingerprint.proxy_id.as_deref(),
                &f.refresh_token,
            )
            .await
            .map_err(|e| ApiError::bad(e.to_string()))?;
        completed(&s, done).await
    } else {
        let p = s
            .auth
            .begin_manual_login_with_identity(
                identity,
                matches!(f.method, Method::Device),
                f.fingerprint.proxy_id.as_deref(),
            )
            .await
            .map_err(|e| ApiError::bad(e.to_string()))?;
        pending(&s, &p)
    }
}
pub async fn relogin(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<Relogin>,
) -> ApiResult {
    if matches!(f.method, Method::RefreshToken) {
        return Err(ApiError::bad("重新登录请选择回调或设备授权"));
    }
    s.storage.require_account(&id).await?;
    let p = s
        .auth
        .begin_manual_login_with_proxy(Some(&id), matches!(f.method, Method::Device), None)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    pending(&s, &p)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Callback {
    state: String,
    callback_url: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingInput {
    state: String,
}
pub async fn callback(State(s): State<AdminState>, Json(f): Json<Callback>) -> ApiResult {
    let done = s
        .auth
        .complete_manual_callback(&f.state, &f.callback_url)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    completed(&s, done).await
}
pub async fn poll(State(s): State<AdminState>, Json(f): Json<PendingInput>) -> ApiResult {
    match s
        .auth
        .poll_device_login(&f.state)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?
    {
        Some(done) => completed(&s, done).await,
        None => Ok(Json(json!({"status":"pending"}))),
    }
}
pub async fn cancel(State(s): State<AdminState>, Json(f): Json<PendingInput>) -> ApiResult {
    s.auth.cancel_draft_login(&f.state).await;
    Ok(Json(json!({"status":"cancelled"})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credit_requests_generate_identifiers_and_preserve_explicit_retry_identity() {
        let input =
            json!({"credit_id":"credit","redeem_request_id":uuid::Uuid::new_v4().to_string()});
        let first = credit_payload(serde_json::from_value(input.clone()).unwrap())
            .ok()
            .unwrap();
        let retried = credit_payload(serde_json::from_value(input).unwrap())
            .ok()
            .unwrap();
        assert_eq!(first, retried);
        let generated = credit_payload(serde_json::from_value(json!({})).unwrap())
            .ok()
            .unwrap();
        assert!(uuid::Uuid::parse_str(generated["redeem_request_id"].as_str().unwrap()).is_ok());
        assert!(
            credit_payload(serde_json::from_value(json!({"redeem_request_id":"bad"})).unwrap())
                .is_err()
        );
        assert!(serde_json::from_value::<CreditInput>(json!({"action":"create_task"})).is_err());
    }
}

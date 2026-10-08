//! ChatGPT supplier management protocol. No Grok branches belong in this module.
use crate::AdminState;
use crate::rest::error::{ApiError, ApiResult};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use codex2api_accounts::{AccountIdentity, HostRuntime, new_installation_id};
use codex2api_auth::{CompletedLogin, LoginFlow};
use codex2api_storage::{SupplierAccount, SupplierInfoSection};
use codex2api_upstream::BackendEndpoint as E;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
pub(crate) mod billing;
mod services;
pub(crate) use services::quota;
pub(crate) async fn subscription_expiration(
    s: &AdminState,
    id: &str,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, ApiError> {
    Ok(s.storage
        .load_supplier_tokens(id)
        .await?
        .and_then(|t| t.id_token)
        .and_then(|t| {
            codex2api_auth::parse_chatgpt_subscription_expiration(&t)
                .ok()
                .flatten()
        }))
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
    pub(crate) fn from_account(a: &SupplierAccount) -> Self {
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
pub async fn official_rows(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Query(q): Query<codex2api_storage::ListQuery>,
) -> ApiResult {
    if s.storage.require_account(&id).await?.provider_id != codex2api_core::CHATGPT {
        return Err(ApiError::missing());
    }
    Ok(Json(json!(s.storage.chatgpt_official_page(&id, &q).await?)))
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
    let account = s.storage.require_account(&id).await?;
    if q.section == "details" {
        let refresh_error = if q.refresh {
            let revision = s.storage.supplier_auth_revision(&id).await?;
            let observation = s.storage.supplier_health(&id).await?.cooldown_revision;
            let result =
                async { s.upstream.get(&id).await?.refresh_workspace_details().await }.await;
            match result {
                Ok(_) => None,
                Err(error) => {
                    services::observe_request_failure(&s, &id, revision, observation, &error)
                        .await
                        .map_err(ApiError::upstream)?;
                    Some(error.to_string())
                }
            }
        } else {
            None
        };
        let snapshot = s.storage.supplier_routing_snapshot(&id).await?;
        let current_revision = s.storage.supplier_auth_revision(&id).await?;
        let mut routing =
            json!({"status":"not_observed","backend_origin":null,"constraint":null,"message":null});
        let (mut value, observed_at) = match snapshot {
            Some((snapshot, revision)) => {
                match codex2api_upstream::WorkspaceRoute::from_accounts(
                    &snapshot.value,
                    account.chatgpt_account_id.as_deref().unwrap_or(""),
                ) {
                    Ok(route) => {
                        routing["backend_origin"] = route.backend_origin.into();
                        routing["constraint"] = route.account_routing_override.into();
                        routing["status"] = if revision.is_some() && revision == current_revision {
                            "ready"
                        } else {
                            "stale"
                        }
                        .into();
                    }
                    Err(error) => {
                        routing["status"] = "invalid".into();
                        routing["message"] = error.to_string().into();
                    }
                }
                (snapshot.value, Some(snapshot.observed_at))
            }
            None => (Value::Null, None),
        };
        if let Some(object) = value.as_object_mut() {
            object.remove("accounts");
        }
        return Ok(Json(
            json!({"value":value,"observed_at":observed_at,"refresh_error":refresh_error,"routing":routing}),
        ));
    }
    let (section, endpoint) = match q.section.as_str() {
        "quota" => (SupplierInfoSection::Quota, E::Usage),
        "usage" => (SupplierInfoSection::Usage, E::Profile),
        "details" => (SupplierInfoSection::Details, E::Accounts),
        "credits" => (SupplierInfoSection::Credits, E::Credits),
        _ => return Err(ApiError::bad("未知官方数据类别")),
    };
    let result = if section == SupplierInfoSection::Quota {
        services::quota(&s, &id, q.refresh).await
    } else {
        s.supplier_cache
            .get_or_fetch(&id, section, q.refresh, || async {
                services::request(&s, &id, endpoint, &HashMap::new(), None, None).await
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
    if section == SupplierInfoSection::Credits
        && let Some(object) = value["value"].as_object_mut()
    {
        object.remove("credits");
    }
    if section == SupplierInfoSection::Quota {
        value["quota"] = crate::quota::summary(&s.storage, &id, &snapshot).await?;
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
    let value = services::request(&s, &id, E::ConsumeCredit, &HashMap::new(), None, Some(body))
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
    #[serde(default)]
    independent_fingerprint: bool,
    fingerprint: Fingerprint,
    #[serde(default)]
    refresh_token: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relogin {
    method: Method,
    #[serde(default)]
    refresh_token: String,
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
    match services::request(s, &done.account.id, E::Profile, &HashMap::new(), None, None).await {
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
        json!({"status":"complete","supplier_id":done.account.id,"reused_existing":done.reused_existing}),
    ))
}
pub async fn start(State(s): State<AdminState>, Json(f): Json<StartInput>) -> ApiResult {
    f.fingerprint.validate_proxy(&s).await?;
    let mut identity = f
        .fingerprint
        .identity(uuid::Uuid::new_v4().to_string(), new_installation_id())?;
    if f.independent_fingerprint {
        if !matches!(f.method, Method::RefreshToken) {
            return Err(ApiError::bad("独立指纹仅用于 RT 批量创建"));
        }
        let timezone = identity.http_fingerprint.timezone.clone();
        identity = AccountIdentity::new(
            identity.account_id,
            identity.installation_id,
            HostRuntime::generate(),
        );
        identity.http_fingerprint.timezone = timezone;
    }
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
        let done = s
            .auth
            .relogin_with_refresh_token(&id, &f.refresh_token)
            .await
            .map_err(|e| ApiError::bad(e.to_string()))?;
        return completed(&s, done).await;
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

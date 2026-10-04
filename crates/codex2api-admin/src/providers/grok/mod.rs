//! Grok supplier administration. All auth and official-data requests stay in this channel.
use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use codex2api_accounts::providers::grok::GrokIdentity;
use codex2api_storage::{QuotaSnapshot, SupplierAccount, SupplierInfoSection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fingerprint {
    os_type: String,
    os_version: String,
    arch: String,
    #[serde(default)]
    terminal: String,
    proxy_id: Option<String>,
    #[serde(default)]
    timezone: String,
}
pub fn fingerprint_view(account: &SupplierAccount) -> Value {
    let identity = GrokIdentity::from_account(account);
    json!({"os_type":identity.os_type,"os_version":identity.os_version,"arch":identity.arch,"terminal":"grok-pager","timezone":identity.timezone.unwrap_or_default(),"proxy_id":account.proxy_id})
}
impl Fingerprint {
    fn identity(
        &self,
        id: String,
        installation: String,
        independent: bool,
    ) -> Result<GrokIdentity, ApiError> {
        if !self.timezone.trim().is_empty() && self.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(ApiError::bad("请输入有效的 IANA 时区"));
        }
        if ![
            "Windows", "Mac OS", "Ubuntu", "Debian", "Fedora", "Arch", "windows", "macos", "linux",
        ]
        .contains(&self.os_type.as_str())
            || !["x86_64", "aarch64"].contains(&self.arch.as_str())
        {
            return Err(ApiError::bad("请选择 Grok 支持的操作系统与架构"));
        }
        let timezone = (!self.timezone.trim().is_empty()).then(|| self.timezone.trim().to_owned());
        if independent {
            let mut identity = GrokIdentity::generate();
            identity.timezone = timezone;
            return Ok(identity);
        }
        Ok(GrokIdentity::new(
            id,
            installation,
            self.os_type.clone(),
            self.os_version.clone(),
            self.arch.clone(),
            timezone,
        ))
    }
}
pub async fn setup() -> ApiResult {
    let identity = GrokIdentity::generate();
    Ok(Json(
        json!({"fingerprint":{"os_type":identity.os_type,"os_version":identity.os_version,"arch":identity.arch,"terminal":"grok-pager","proxy_id":null,"timezone":""}}),
    ))
}
pub async fn fingerprint(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<Fingerprint>,
) -> ApiResult {
    let mut account = s.storage.require_account(&id).await?;
    if account.provider_id != codex2api_core::GROK {
        return Err(ApiError::bad("提供商不匹配"));
    }
    if let Some(proxy) = f.proxy_id.as_deref().filter(|v| !v.is_empty()) {
        s.storage.require_outbound_proxy(proxy).await?;
    }
    let identity = f.identity(id.clone(), account.installation_id.clone(), false)?;
    account.os_type = identity.os_type.clone();
    account.os_version = identity.os_version.clone();
    account.arch = identity.arch.clone();
    account.originator = identity.originator.clone();
    account.user_agent = identity.user_agent.clone();
    account.http_fingerprint_json = identity
        .fingerprint_json()
        .map_err(|_| ApiError::bad("指纹无效"))?;
    account.proxy_id = f.proxy_id.filter(|v| !v.is_empty());
    s.storage.save_account_fingerprint(&account).await?;
    s.upstream
        .grok()
        .auth()
        .account_http(&id)
        .await
        .map_err(|_| ApiError::upstream("指纹已保存，Grok 连接重建失败"))?;
    Ok(Json(fingerprint_view(&account)))
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Method {
    Callback,
    Device,
    RefreshToken,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Start {
    method: Method,
    fingerprint: Fingerprint,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    independent_fingerprint: bool,
}
pub async fn start(State(s): State<AdminState>, Json(f): Json<Start>) -> ApiResult {
    if f.independent_fingerprint && !matches!(f.method, Method::RefreshToken) {
        return Err(ApiError::bad("独立指纹仅用于 RT 批量创建"));
    }
    let identity = f.fingerprint.identity(
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
        f.independent_fingerprint,
    )?;
    let proxy = f.fingerprint.proxy_id.as_deref().filter(|v| !v.is_empty());
    let auth = s.upstream.grok().auth();
    if matches!(f.method, Method::RefreshToken) {
        let draft = identity.account_id.clone();
        let account = auth
            .grok_login_rt(identity, proxy, None, &f.refresh_token)
            .await
            .map_err(|e| ApiError::bad(e.to_string()))?;
        s.supplier_cache
            .invalidate(&account.id, SupplierInfoSection::Quota)
            .await;
        return authorized(&s, &account.id, account.id != draft).await;
    }
    Ok(Json(
        auth.grok_begin(identity, proxy, None, matches!(f.method, Method::Device))
            .await
            .map_err(|e| ApiError::bad(e.to_string()))?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relogin {
    method: Method,
    #[serde(default)]
    refresh_token: String,
}
pub async fn relogin(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<Relogin>,
) -> ApiResult {
    let account = s.storage.require_account(&id).await?;
    if account.provider_id != codex2api_core::GROK {
        return Err(ApiError::bad("提供商不匹配"));
    }
    let auth = s.upstream.grok().auth();
    let identity = GrokIdentity::from_account(&account);
    if matches!(f.method, Method::RefreshToken) {
        auth.grok_login_rt(
            identity,
            account.proxy_id.as_deref(),
            Some(&id),
            &f.refresh_token,
        )
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
        s.supplier_cache
            .invalidate(&id, SupplierInfoSection::Quota)
            .await;
        return authorized(&s, &id, true).await;
    }
    Ok(Json(
        auth.grok_begin(
            identity,
            account.proxy_id.as_deref(),
            Some(&id),
            matches!(f.method, Method::Device),
        )
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    state: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Callback {
    state: String,
    #[serde(default)]
    callback_url: Option<String>,
    #[serde(default)]
    code: Option<String>,
}
async fn complete(s: &AdminState, state: &str, callback: Option<&str>) -> ApiResult {
    match s
        .upstream
        .grok()
        .auth()
        .grok_complete(state, callback)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?
    {
        Some(account) => {
            s.supplier_cache
                .invalidate(&account.id, SupplierInfoSection::Quota)
                .await;
            authorized(s, &account.id, false).await
        }
        None => Ok(Json(json!({"status":"pending"}))),
    }
}
pub async fn callback(State(s): State<AdminState>, Json(f): Json<Callback>) -> ApiResult {
    let input = match (f.code, f.callback_url) {
        (Some(value), None) | (None, Some(value)) => value,
        _ => return Err(ApiError::bad("请输入一份 Grok 授权代码或回调链接")),
    };
    complete(&s, &f.state, Some(&input)).await
}
pub async fn poll(State(s): State<AdminState>, Json(f): Json<Pending>) -> ApiResult {
    complete(&s, &f.state, None).await
}
pub async fn cancel(State(s): State<AdminState>, Json(f): Json<Pending>) -> ApiResult {
    s.upstream.grok().auth().grok_cancel(&f.state).await;
    Ok(Json(json!({"status":"cancelled"})))
}
pub fn oauth_routes() -> Router<AdminState> {
    Router::new()
        .route("/setup", get(setup))
        .route("/start", post(start))
        .route("/callback", post(callback))
        .route("/poll", post(poll))
        .route("/cancel", post(cancel))
}

async fn authorized(s: &AdminState, id: &str, reused: bool) -> ApiResult {
    let _ = s;
    Ok(Json(
        json!({"status":"complete","supplier_id":id,"reused_existing":reused}),
    ))
}
pub async fn model_catalog(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    if s.storage.require_account(&id).await?.provider_id != codex2api_core::GROK {
        return Err(ApiError::bad("提供商不匹配"));
    }
    Ok(Json(s.storage.grok_catalog(&id).await?))
}
pub async fn refresh_profile(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    s.upstream
        .grok()
        .auth()
        .refresh_profile(&id)
        .await
        .map_err(|e| ApiError::upstream(e.to_string()))?;
    Ok(Json(json!({"ok":true})))
}
pub async fn sync_model_catalog(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    Ok(Json(
        s.upstream
            .grok()
            .sync_models(&id)
            .await
            .map_err(|e| ApiError::upstream(e.to_string()))?,
    ))
}
pub async fn sync_all_models(State(s): State<AdminState>) -> ApiResult {
    let accounts = s
        .storage
        .list_accounts()
        .await?
        .into_iter()
        .filter(|a| {
            a.provider_id == codex2api_core::GROK
                && a.status == codex2api_storage::SupplierStatus::Active
        })
        .collect::<Vec<_>>();
    if accounts.is_empty() {
        return Err(ApiError::bad("请先授权至少一个 Grok 供应账户"));
    }
    let mut results = Vec::new();
    for account in accounts {
        let result = s.upstream.grok().sync_models(&account.id).await;
        results.push(match result {Ok(value)=>json!({"supplier_id":account.id,"success":true,"models":value["items"].as_array().map(Vec::len).unwrap_or(0)}),Err(error)=>{tracing::warn!(%error,"Grok model discovery failed");json!({"supplier_id":account.id,"success":false,"error":"模型目录同步失败，请检查授权或网络"})}});
    }
    Ok(Json(json!({"items":results})))
}

pub async fn quota(s: &AdminState, id: &str, refresh: bool) -> Result<QuotaSnapshot, String> {
    s.supplier_cache
        .get_or_fetch(id, SupplierInfoSection::Quota, refresh, || async {
            codex2api_upstream::grok::quota_snapshot(
                s.upstream
                    .grok()
                    .grok_json(id, "billing?format=credits")
                    .await
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        })
        .await
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
    let (section, path) = match q.section.as_str() {
        "quota" => (SupplierInfoSection::Quota, "billing?format=credits"),
        "details" => (SupplierInfoSection::Details, "user?include=subscription"),
        "usage" => (SupplierInfoSection::Usage, "settings"),
        "credits" => (SupplierInfoSection::Credits, "billing?format=credits"),
        _ => return Err(ApiError::bad("未知 Grok 数据类别")),
    };
    let result = if section == SupplierInfoSection::Quota {
        quota(&s, &id, q.refresh).await
    } else {
        s.supplier_cache
            .get_or_fetch(&id, section, q.refresh, || async {
                if section == SupplierInfoSection::Details {
                    return s
                        .upstream
                        .grok()
                        .auth()
                        .refresh_profile(&id)
                        .await
                        .map_err(|e| e.to_string());
                }
                s.upstream
                    .grok()
                    .grok_json(&id, path)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
    };
    let (snapshot, error) = match result {
        Ok(snapshot) => (snapshot, None),
        Err(error) => (
            s.storage
                .get_supplier_info(&id, section)
                .await?
                .ok_or_else(|| ApiError::upstream(&error))?,
            Some(error),
        ),
    };
    let summary = if section == SupplierInfoSection::Quota {
        Some(crate::quota::summary(&s.storage, &id, &snapshot).await?)
    } else {
        None
    };
    Ok(Json(
        json!({"value":snapshot.value,"observed_at":snapshot.observed_at,"refresh_error":error,"quota":summary}),
    ))
}

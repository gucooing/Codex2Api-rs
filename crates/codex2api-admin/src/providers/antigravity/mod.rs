//! Antigravity supplier administration: Google OAuth callback and RT login, frozen
//! platform tokens, and the official onboarding and quota reads of this channel only.
pub(crate) mod transfer;
use crate::{
    AdminState,
    rest::error::{ApiError, ApiResult},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use codex2api_storage::{QuotaSnapshot, SupplierAccount, SupplierInfoSection};
use codex2api_suppliers::antigravity::{
    AntigravityIdentity,
    identity::{ARCHS, OS_TYPES},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fingerprint {
    os_type: String,
    #[serde(default)]
    os_version: String,
    arch: String,
    #[serde(default)]
    terminal: String,
    proxy_id: Option<String>,
    #[serde(default)]
    timezone: String,
}
pub fn fingerprint_view(account: &SupplierAccount) -> Result<Value, ApiError> {
    let identity =
        AntigravityIdentity::from_account(account).map_err(|_| ApiError::bad("账户指纹无效"))?;
    Ok(
        json!({"os_type":identity.os_type,"os_version":identity.os_version,"arch":identity.arch,"terminal":"",
            "timezone":identity.timezone.unwrap_or_default(),"proxy_id":account.proxy_id}),
    )
}
impl Fingerprint {
    fn identity(
        &self,
        id: String,
        installation: String,
        os_version: &str,
        independent: bool,
    ) -> Result<AntigravityIdentity, ApiError> {
        if !self.timezone.trim().is_empty() && self.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(ApiError::bad("请输入有效的 IANA 时区"));
        }
        if !OS_TYPES.contains(&self.os_type.as_str()) || !ARCHS.contains(&self.arch.as_str()) {
            return Err(ApiError::bad("请选择 Antigravity 支持的操作系统与架构"));
        }
        let timezone = (!self.timezone.trim().is_empty()).then(|| self.timezone.trim().to_owned());
        if independent {
            let mut identity = AntigravityIdentity::generate();
            identity.timezone = timezone;
            return Ok(identity);
        }
        let version = if self.os_version.trim().is_empty() {
            os_version
        } else {
            self.os_version.trim()
        };
        if version.is_empty()
            || version.len() > 64
            || !version.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(ApiError::bad("系统版本格式无效"));
        }
        Ok(AntigravityIdentity::new(
            id,
            installation,
            self.os_type.clone(),
            version.to_owned(),
            self.arch.clone(),
            timezone,
        ))
    }
}
pub async fn setup() -> ApiResult {
    let identity = AntigravityIdentity::generate();
    Ok(Json(
        json!({"fingerprint":{"os_type":identity.os_type,"os_version":identity.os_version,"arch":identity.arch,"terminal":"","proxy_id":null,"timezone":""}}),
    ))
}
pub async fn fingerprint(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<Fingerprint>,
) -> ApiResult {
    let mut account = s.storage.require_account(&id).await?;
    if account.provider_id != codex2api_common::ANTIGRAVITY {
        return Err(ApiError::bad("提供商不匹配"));
    }
    if let Some(proxy) = f.proxy_id.as_deref().filter(|v| !v.is_empty()) {
        s.storage.require_outbound_proxy(proxy).await?;
    }
    let identity = f.identity(
        id.clone(),
        account.installation_id.clone(),
        &account.os_version,
        false,
    )?;
    account.os_type = identity.os_type.clone();
    account.os_version = identity.os_version.clone();
    account.arch = identity.arch.clone();
    account.http_fingerprint_json = identity
        .fingerprint_json()
        .map_err(|_| ApiError::bad("指纹无效"))?;
    account.proxy_id = f.proxy_id.filter(|v| !v.is_empty());
    s.storage.save_account_fingerprint(&account).await?;
    s.upstream
        .antigravity()
        .auth()
        .account_http(&id)
        .await
        .map_err(|_| ApiError::upstream("指纹已保存，Antigravity 连接重建失败"))?;
    Ok(Json(fingerprint_view(&account)?))
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Method {
    Callback,
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
fn authorized(id: &str, reused: bool) -> ApiResult {
    Ok(Json(
        json!({"status":"complete","supplier_id":id,"reused_existing":reused}),
    ))
}
pub async fn start(State(s): State<AdminState>, Json(f): Json<Start>) -> ApiResult {
    if f.independent_fingerprint && !matches!(f.method, Method::RefreshToken) {
        return Err(ApiError::bad("独立指纹仅用于 RT 批量创建"));
    }
    let identity = f.fingerprint.identity(
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
        "",
        f.independent_fingerprint,
    )?;
    let proxy = f.fingerprint.proxy_id.as_deref().filter(|v| !v.is_empty());
    let auth = s.upstream.antigravity().auth();
    if matches!(f.method, Method::RefreshToken) {
        let draft = identity.account_id.clone();
        let account = auth
            .login_rt(identity, proxy, None, &f.refresh_token)
            .await
            .map_err(|e| ApiError::bad(e.to_string()))?;
        s.supplier_cache.invalidate_all(&account.id).await;
        return authorized(&account.id, account.id != draft);
    }
    Ok(Json(
        auth.begin(identity, proxy, None)
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
    if account.provider_id != codex2api_common::ANTIGRAVITY {
        return Err(ApiError::bad("提供商不匹配"));
    }
    let auth = s.upstream.antigravity().auth();
    let identity =
        AntigravityIdentity::from_account(&account).map_err(|_| ApiError::bad("账户指纹无效"))?;
    if matches!(f.method, Method::RefreshToken) {
        auth.login_rt(
            identity,
            account.proxy_id.as_deref(),
            Some(&id),
            &f.refresh_token,
        )
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
        s.supplier_cache.invalidate_all(&id).await;
        return authorized(&id, true);
    }
    Ok(Json(
        auth.begin(identity, account.proxy_id.as_deref(), Some(&id))
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
pub async fn callback(State(s): State<AdminState>, Json(f): Json<Callback>) -> ApiResult {
    let input = match (f.code, f.callback_url) {
        (Some(value), None) | (None, Some(value)) => value,
        _ => return Err(ApiError::bad("请输入一份 Google 授权代码或回调链接")),
    };
    let account = s
        .upstream
        .antigravity()
        .auth()
        .complete(&f.state, &input)
        .await
        .map_err(|e| ApiError::bad(e.to_string()))?;
    s.supplier_cache.invalidate_all(&account.id).await;
    authorized(&account.id, false)
}
pub async fn cancel(State(s): State<AdminState>, Json(f): Json<Pending>) -> ApiResult {
    s.upstream.antigravity().auth().cancel(&f.state).await;
    Ok(Json(json!({"status":"cancelled"})))
}
pub fn oauth_routes() -> Router<AdminState> {
    Router::new()
        .route("/setup", get(setup))
        .route("/start", post(start))
        .route("/callback", post(callback))
        .route("/cancel", post(cancel))
}
pub async fn refresh_profile(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    s.upstream
        .antigravity()
        .auth()
        .refresh_profile(&id)
        .await
        .map_err(|e| ApiError::upstream(e.to_string()))?;
    Ok(Json(json!({"ok":true})))
}
/// The official per-model quota read; it also records exhausted and recovered models.
pub async fn quota(s: &AdminState, id: &str, refresh: bool) -> Result<QuotaSnapshot, String> {
    s.supplier_cache
        .get_or_fetch(id, SupplierInfoSection::Quota, refresh, || async {
            s.upstream
                .antigravity()
                .quota(id)
                .await
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
    let result = match q.section.as_str() {
        "quota" => quota(&s, &id, q.refresh).await,
        "details" => {
            s.supplier_cache
                .get_or_fetch(&id, SupplierInfoSection::Details, q.refresh, || async {
                    s.upstream
                        .antigravity()
                        .auth()
                        .refresh_profile(&id)
                        .await
                        .map_err(|e| e.to_string())
                })
                .await
        }
        _ => return Err(ApiError::bad("Antigravity 不提供该数据类别")),
    };
    let section = if q.section == "quota" {
        SupplierInfoSection::Quota
    } else {
        SupplierInfoSection::Details
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
    Ok(Json(
        json!({"value":snapshot.value,"observed_at":snapshot.observed_at,"refresh_error":error,"quota":Value::Null}),
    ))
}

pub(crate) fn request_identity(
    account: &codex2api_storage::SupplierAccount,
) -> Result<(&'static str, String), ApiError> {
    let identity =
        AntigravityIdentity::from_account(account).map_err(|_| ApiError::bad("账户指纹无效"))?;
    Ok((
        codex2api_common::version::antigravity::ENVELOPE_USER_AGENT,
        identity.user_agent(),
    ))
}

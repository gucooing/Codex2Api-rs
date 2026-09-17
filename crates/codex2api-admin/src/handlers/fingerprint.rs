use crate::{AdminState, response, session, views};
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use codex2api_accounts::{AccountIdentity, HostRuntime};
use codex2api_storage::Account;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FingerprintForm {
    pub csrf: String,
    pub os_type: String,
    pub os_version: String,
    pub arch: String,
    pub terminal: String,
    #[serde(default)]
    pub proxy_id: String,
    #[serde(default)]
    pub timezone: String,
}

impl FingerprintForm {
    pub(crate) fn from_account(account: &Account, csrf: &str) -> Self {
        let identity = AccountIdentity::from_account(account);
        let mut form = Self::from_identity(&identity, csrf);
        form.proxy_id = account.proxy_id.clone().unwrap_or_default();
        form
    }

    pub(crate) fn from_identity(identity: &AccountIdentity, csrf: &str) -> Self {
        let user_agent = identity.official_user_agent();
        Self {
            proxy_id: String::new(),
            timezone: identity
                .http_fingerprint
                .timezone
                .clone()
                .unwrap_or_default(),
            csrf: csrf.to_string(),
            os_type: identity.os_type.clone(),
            os_version: identity.os_version.clone(),
            arch: identity.arch.clone(),
            terminal: user_agent
                .split_once(") ")
                .map_or("unknown", |(_, terminal)| terminal)
                .to_string(),
        }
    }

    fn validate(&self) -> Result<(), String> {
        let timezone = self.timezone.trim();
        if !timezone.is_empty() && timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err("请输入有效的 IANA 时区，例如 Asia/Taipei".into());
        }
        for (label, value, max) in [
            ("操作系统", self.os_type.as_str(), 128),
            ("系统版本", self.os_version.as_str(), 128),
            ("架构", self.arch.as_str(), 128),
            ("终端标识", self.terminal.as_str(), 256),
        ] {
            if value.trim().is_empty() || value.len() > max {
                return Err(format!("{label}不能为空，且不能超过 {max} 个字符"));
            }
            if !value.bytes().all(|b| (b' '..=b'~').contains(&b)) {
                return Err(format!("{label}只能包含可打印的英文字符、数字和符号"));
            }
            if label != "终端标识" && value.contains(['(', ')', ';']) {
                return Err(format!("{label}不能包含括号或分号"));
            }
        }
        Ok(())
    }

    pub(crate) fn build_identity(
        &self,
        account_id: String,
        installation_id: String,
    ) -> Result<AccountIdentity, String> {
        self.validate()?;
        let runtime = HostRuntime {
            originator: codex2api_version::DEFAULT_ORIGINATOR.to_string(),
            user_agent: codex2api_version::official_user_agent(
                self.os_type.trim(),
                self.os_version.trim(),
                self.arch.trim(),
                self.terminal.trim(),
            ),
            os_type: self.os_type.trim().to_string(),
            os_version: self.os_version.trim().to_string(),
            arch: self.arch.trim().to_string(),
        };
        let mut identity = AccountIdentity::new(account_id, installation_id, runtime);
        identity.http_fingerprint.timezone =
            (!self.timezone.trim().is_empty()).then(|| self.timezone.trim().to_string());
        Ok(identity)
    }

    fn apply(&self, account: &mut Account) -> Result<(), String> {
        self.validate()?;
        let timezone = self.timezone.trim();
        let mut fingerprint = AccountIdentity::from_account(account).http_fingerprint;
        fingerprint.timezone = (!timezone.is_empty()).then(|| timezone.to_owned());
        fingerprint.originator = codex2api_version::DEFAULT_ORIGINATOR.to_string();
        fingerprint.installation_id = account.installation_id.clone();
        fingerprint.os_type = self.os_type.trim().to_string();
        fingerprint.os_version = self.os_version.trim().to_string();
        fingerprint.arch = self.arch.trim().to_string();
        fingerprint.user_agent = codex2api_version::official_user_agent(
            &fingerprint.os_type,
            &fingerprint.os_version,
            &fingerprint.arch,
            self.terminal.trim(),
        );
        account.http_fingerprint_json = fingerprint.to_json().map_err(|error| error.to_string())?;
        account.originator = fingerprint.originator;
        account.user_agent = fingerprint.user_agent;
        account.os_type = fingerprint.os_type;
        account.os_version = fingerprint.os_version;
        account.arch = fingerprint.arch;
        account.proxy_id = (!self.proxy_id.is_empty()).then(|| self.proxy_id.clone());
        Ok(())
    }
}

pub async fn save(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<FingerprintForm>,
) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    if form.csrf != super::official::csrf_token(&session.id) {
        return (
            StatusCode::FORBIDDEN,
            Html(views::error_page("请求无效", "请刷新页面后重试")),
        )
            .into_response();
    }
    let mut account = match state.storage.get_account(&id).await {
        Ok(Some(account)) => account,
        Ok(None) => return response::not_found_account(&id),
        Err(error) => return response::storage_error("无法加载指纹配置", error),
    };
    let proxies = match state.storage.list_outbound_proxies().await {
        Ok(proxies) => proxies,
        Err(error) => return response::storage_error("无法加载代理配置", error),
    };
    let result =
        if !form.proxy_id.is_empty() && !proxies.iter().any(|proxy| proxy.id == form.proxy_id) {
            Err("请选择有效的代理".into())
        } else {
            form.apply(&mut account)
        };
    if let Err(error) = result {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CACHE_CONTROL, "no-store")],
            Html(views::account::render(
                &account,
                "fingerprint",
                None,
                Some(&error),
                &views::fingerprint::render(&account, &form, &proxies),
            )),
        )
            .into_response();
    }
    if let Err(error) = state.storage.save_account_fingerprint(&account).await {
        return response::storage_error("无法保存指纹配置", error);
    }
    let reload = state.auth.reload_account_http(&id).await;
    state.upstream.evict(&id).await;
    let (kind, message) = match reload {
        Ok(()) => ("ok", "指纹配置已保存".to_string()),
        Err(error) => (
            "err",
            format!("指纹配置已保存，但请求客户端更新失败：{error}"),
        ),
    };
    Redirect::to(&format!(
        "/admin/accounts/{id}?tab=fingerprint&{kind}={}",
        response::encode_query(&message)
    ))
    .into_response()
}

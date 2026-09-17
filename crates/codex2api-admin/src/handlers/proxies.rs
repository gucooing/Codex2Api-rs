use crate::{AdminState, proxy_checks, response, session, views};
use axum::{
    extract::{Form, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct PageQuery {
    #[serde(default)]
    pub keyword: String,
    #[serde(default)]
    pub protocol: String,
    pub ok: Option<String>,
    pub err: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateForm {
    csrf: String,
    name: String,
    protocol: String,
    host: String,
    port: u16,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
}

impl CreateForm {
    fn proxy_url(&self) -> Result<String, String> {
        if !matches!(
            self.protocol.as_str(),
            "http" | "https" | "socks5" | "socks5h"
        ) || self.port == 0
        {
            return Err("请选择有效协议和 1–65535 之间的端口".into());
        }
        let host = self.host.trim();
        if host.is_empty()
            || host.contains(['/', '\\', '@', '?', '#'])
            || host.chars().any(char::is_whitespace)
        {
            return Err("请填写主机名或 IP，不要包含协议和端口".into());
        }
        let host = match host.parse::<std::net::Ipv6Addr>() {
            Ok(ip) => format!("[{ip}]"),
            Err(_) => host.to_string(),
        };
        url::Host::parse(&host).map_err(|_| "主机名或 IP 无效")?;
        if self.username.len() > 255 || self.password.len() > 255 {
            return Err("用户名和密码均不能超过 255 字节".into());
        }
        let mut url =
            url::Url::parse(&format!("{}://localhost", self.protocol)).map_err(|_| "协议无效")?;
        url.set_host(Some(&host)).map_err(|_| "主机名或 IP 无效")?;
        url.set_port(Some(self.port)).map_err(|_| "端口无效")?;
        url.set_username(&urlencoding::encode(&self.username))
            .map_err(|_| "用户名无效")?;
        if !self.password.is_empty() {
            url.set_password(Some(&urlencoding::encode(&self.password)))
                .map_err(|_| "密码无效")?;
        }
        codex2api_storage::parse_proxy_url(url.as_str()).map_err(|_| "代理配置无效")?;
        Ok(url.to_string())
    }
}

#[derive(Deserialize)]
pub struct CheckForm {
    csrf: String,
}

#[derive(Deserialize)]
pub struct DeleteForm {
    csrf: String,
    #[serde(default)]
    confirm_unbind: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_fields_preserve_ipv6_and_special_credentials() {
        for protocol in ["http", "https", "socks5", "socks5h"] {
            let mut form = CreateForm {
                csrf: String::new(),
                name: "test".into(),
                protocol: protocol.into(),
                host: "::1".into(),
                port: 8080,
                username: "user%@:/".into(),
                password: "p%20@?#密碼".into(),
            };
            let url = url::Url::parse(&form.proxy_url().unwrap()).unwrap();
            assert_eq!(url.host_str(), Some("[::1]"));
            assert_eq!(urlencoding::decode(url.username()).unwrap(), form.username);
            assert_eq!(
                urlencoding::decode(url.password().unwrap()).unwrap(),
                form.password
            );
            for host in ["http://host", "host:8080", "host/path", "user@host"] {
                form.host = host.into();
                assert!(form.proxy_url().is_err(), "{host}");
            }
        }
    }
}

pub async fn page(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    let (proxies, counts) = tokio::join!(
        state.storage.list_outbound_proxies(),
        state.storage.outbound_proxy_account_counts()
    );
    match (proxies, counts) {
        (Ok(mut proxies), Ok(counts)) => {
            let keyword = query.keyword.trim().to_lowercase();
            proxies.retain(|proxy| {
                let protocol = url::Url::parse(&proxy.url)
                    .ok()
                    .map(|url| url.scheme().to_owned())
                    .unwrap_or_default();
                let searchable = format!(
                    "{} {} {} {} {}",
                    proxy.name,
                    proxy.display_url(),
                    proxy.country.as_deref().unwrap_or(""),
                    proxy.region.as_deref().unwrap_or(""),
                    proxy.city.as_deref().unwrap_or("")
                );
                (query.protocol.is_empty() || query.protocol == protocol)
                    && (keyword.is_empty() || searchable.to_lowercase().contains(&keyword))
            });
            (
                [(header::CACHE_CONTROL, "no-store")],
                Html(views::proxies::render(
                    &proxies,
                    &counts,
                    &super::official::csrf_token(&session.id),
                    &query,
                )),
            )
                .into_response()
        }
        (Err(error), _) | (_, Err(error)) => response::storage_error("无法加载代理配置", error),
    }
}

fn redirect(kind: &str, message: &str) -> Response {
    Redirect::to(&format!(
        "/admin/proxies?{kind}={}",
        response::encode_query(message)
    ))
    .into_response()
}

pub async fn create(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Form(form): Form<CreateForm>,
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
    let url = match form.proxy_url() {
        Ok(url) => url,
        Err(error) => return redirect("err", &error),
    };
    match state.storage.create_outbound_proxy(&form.name, &url).await {
        Ok(_) => redirect("ok", "代理已添加"),
        Err(codex2api_storage::StorageError::InvalidProxy) => {
            redirect("err", "请填写有效的代理名称和配置")
        }
        Err(error) => response::storage_error("无法添加代理", error),
    }
}

fn check_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(serde_json::json!({"error":message})),
    )
        .into_response()
}

pub async fn edit(State(state): State<AdminState>, Path(id): Path<String>) -> Response {
    let proxy = match state.storage.require_outbound_proxy(&id).await {
        Ok(proxy) => proxy,
        Err(codex2api_storage::StorageError::ProxyNotFound) => {
            return check_error(StatusCode::NOT_FOUND, "代理不存在");
        }
        Err(_) => return check_error(StatusCode::INTERNAL_SERVER_ERROR, "无法加载代理配置"),
    };
    let url = match codex2api_storage::parse_proxy_url(&proxy.url) {
        Ok(url) => url,
        Err(_) => return check_error(StatusCode::BAD_REQUEST, "代理配置无效"),
    };
    let username = urlencoding::decode(url.username())
        .unwrap_or_default()
        .into_owned();
    let password = urlencoding::decode(url.password().unwrap_or(""))
        .unwrap_or_default()
        .into_owned();
    let counts = match state.storage.outbound_proxy_account_counts().await {
        Ok(counts) => counts,
        Err(_) => return check_error(StatusCode::INTERNAL_SERVER_ERROR, "无法加载绑定账户数量"),
    };
    ([(header::CACHE_CONTROL, "no-store")], axum::Json(serde_json::json!({
        "name":proxy.name, "protocol":url.scheme(), "host":url.host_str().unwrap_or(""),
        "port":url.port_or_known_default().unwrap_or(1080), "username":username, "password":password,
        "account_count": counts.get(&id).copied().unwrap_or(0),
    }))).into_response()
}

pub async fn update(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<CreateForm>,
) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    if form.csrf != super::official::csrf_token(&session.id) {
        return check_error(StatusCode::FORBIDDEN, "请刷新页面后重试");
    }
    let url = match form.proxy_url() {
        Ok(url) => url,
        Err(error) => return redirect("err", &error),
    };
    match state
        .storage
        .update_outbound_proxy(&id, &form.name, &url)
        .await
    {
        Ok(_) => redirect("ok", "代理已更新"),
        Err(codex2api_storage::StorageError::ProxyNotFound) => {
            check_error(StatusCode::NOT_FOUND, "代理不存在")
        }
        Err(codex2api_storage::StorageError::InvalidProxy) => {
            redirect("err", "请填写有效的代理名称和配置")
        }
        Err(error) => response::storage_error("无法保存代理配置", error),
    }
}

pub async fn delete(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<DeleteForm>,
) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return check_error(StatusCode::UNAUTHORIZED, "登录已失效，请重新登录");
    };
    if form.csrf != super::official::csrf_token(&session.id) {
        return check_error(StatusCode::FORBIDDEN, "请刷新页面后重试");
    }
    match state
        .storage
        .delete_outbound_proxy(&id, form.confirm_unbind)
        .await
    {
        Ok(true) => (
            [(header::CACHE_CONTROL, "no-store")],
            axum::Json(serde_json::json!({"deleted":true})),
        )
            .into_response(),
        Ok(false) => check_error(StatusCode::NOT_FOUND, "代理不存在"),
        Err(codex2api_storage::StorageError::ProxyHasBindings(count)) => (
            StatusCode::CONFLICT,
            [(header::CACHE_CONTROL, "no-store")],
            axum::Json(serde_json::json!({"confirmation_required": true, "account_count": count})),
        )
            .into_response(),
        Err(_) => check_error(StatusCode::INTERNAL_SERVER_ERROR, "无法删除代理"),
    }
}

pub async fn check(
    State(state): State<AdminState>,
    Path((id, action)): Path<(String, String)>,
    headers: HeaderMap,
    Form(form): Form<CheckForm>,
) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return check_error(StatusCode::UNAUTHORIZED, "登录已失效，请重新登录");
    };
    if form.csrf != super::official::csrf_token(&session.id) {
        return check_error(StatusCode::FORBIDDEN, "请刷新页面后重试");
    }
    if !matches!(action.as_str(), "test" | "quality" | "timezone") {
        return check_error(StatusCode::NOT_FOUND, "检测类型不存在");
    }
    let proxy = match state.storage.require_outbound_proxy(&id).await {
        Ok(proxy) => proxy,
        Err(codex2api_storage::StorageError::ProxyNotFound) => {
            return check_error(StatusCode::NOT_FOUND, "代理不存在");
        }
        Err(_) => return check_error(StatusCode::INTERNAL_SERVER_ERROR, "无法加载代理配置"),
    };
    let saved = if action == "test" || action == "timezone" {
        state
            .storage
            .save_proxy_connection_check(&proxy, &proxy_checks::connection(&proxy).await)
            .await
    } else {
        state
            .storage
            .save_proxy_quality_check(&proxy, &proxy_checks::quality(&proxy).await)
            .await
    };
    let proxy = match saved {
        Ok(proxy) => proxy,
        Err(codex2api_storage::StorageError::ProxyChanged) => {
            return check_error(StatusCode::CONFLICT, "代理配置已更改，请重新检测");
        }
        Err(_) => return check_error(StatusCode::INTERNAL_SERVER_ERROR, "无法保存检测结果"),
    };
    if action == "timezone" {
        if proxy.connection_ok != Some(true) {
            return check_error(
                StatusCode::BAD_GATEWAY,
                proxy
                    .connection_error
                    .as_deref()
                    .unwrap_or("无法查询代理时区"),
            );
        }
        return match proxy.timezone {
            Some(timezone) => (
                [(header::CACHE_CONTROL, "no-store")],
                axum::Json(serde_json::json!({"timezone": timezone})),
            )
                .into_response(),
            None => check_error(StatusCode::BAD_GATEWAY, "未获取到该代理出口地区的有效时区"),
        };
    }
    let counts = match state.storage.outbound_proxy_account_counts().await {
        Ok(counts) => counts,
        Err(_) => return check_error(StatusCode::INTERNAL_SERVER_ERROR, "无法加载账户数量"),
    };
    ([(header::CACHE_CONTROL, "no-store")], axum::Json(serde_json::json!({"row":views::proxies::row(&proxy, counts.get(&id).copied().unwrap_or(0))}))).into_response()
}

use super::error::{ApiError, ApiResult, ok};
use crate::{AdminState, proxy_checks};
use axum::{
    Json,
    extract::{Path, State},
};
use codex2api_storage::OutboundProxy;
use serde::Deserialize;
use serde_json::{Value, json};
fn dto(p: &OutboundProxy, count: i64) -> Value {
    let url = url::Url::parse(&p.url).ok();
    json!({"id":p.id,"name":p.name,"protocol":url.as_ref().map(|u|u.scheme()),"host":url.as_ref().and_then(|u|u.host_str()),"port":url.as_ref().and_then(|u|u.port_or_known_default()).unwrap_or(1080),"username":url.as_ref().map(|u|urlencoding::decode(u.username()).unwrap_or_default().into_owned()),"has_password":url.as_ref().is_some_and(|u|u.password().is_some()),"display_url":p.display_url(),"account_count":count,"exit_ip":p.exit_ip,"country_code":p.country_code,"country":p.country,"region":p.region,"city":p.city,"timezone":p.timezone,"connection_ok":p.connection_ok,"connection_latency_ms":p.connection_latency_ms,"connection_error":p.connection_error,"connection_checked_at":p.connection_checked_at,"quality_ok":p.quality_ok,"quality_latency_ms":p.quality_latency_ms,"quality_http_status":p.quality_http_status,"quality_error":p.quality_error,"quality_checked_at":p.quality_checked_at})
}
pub async fn list(State(s): State<AdminState>) -> ApiResult {
    let counts = s.storage.outbound_proxy_account_counts().await?;
    Ok(Json(
        json!({"items":s.storage.list_outbound_proxies().await?.iter().map(|p|dto(p,counts.get(&p.id).copied().unwrap_or(0))).collect::<Vec<_>>()}),
    ))
}
pub async fn detail(State(s): State<AdminState>, Path(id): Path<String>) -> ApiResult {
    let p = s.storage.require_outbound_proxy(&id).await?;
    let counts = s.storage.outbound_proxy_account_counts().await?;
    Ok(Json(dto(&p, counts.get(&id).copied().unwrap_or(0))))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    name: String,
    protocol: String,
    host: String,
    port: u16,
    #[serde(default)]
    username: String,
    password: Option<String>,
}
impl Input {
    fn url(&self, password: &str) -> Result<String, ApiError> {
        if !matches!(
            self.protocol.as_str(),
            "http" | "https" | "socks5" | "socks5h"
        ) || self.port == 0
        {
            return Err(ApiError::bad("代理协议或端口无效"));
        }
        let host = self
            .host
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']');
        if host.is_empty()
            || host.contains(['/', '\\', '@', '?', '#'])
            || host.chars().any(char::is_whitespace)
        {
            return Err(ApiError::bad("请输入不含协议和端口的主机名或 IP"));
        }
        let host = match host.parse::<std::net::Ipv6Addr>() {
            Ok(ip) => format!("[{ip}]"),
            Err(_) => host.into(),
        };
        url::Host::parse(&host).map_err(|_| ApiError::bad("主机名或 IP 无效"))?;
        if self.username.len() > 255 || password.len() > 255 {
            return Err(ApiError::bad("代理用户名或密码过长"));
        }
        let mut url = url::Url::parse(&format!("{}://localhost", self.protocol)).unwrap();
        url.set_host(Some(&host))
            .map_err(|_| ApiError::bad("主机无效"))?;
        url.set_port(Some(self.port))
            .map_err(|_| ApiError::bad("端口无效"))?;
        url.set_username(&urlencoding::encode(&self.username))
            .map_err(|_| ApiError::bad("用户名无效"))?;
        if !password.is_empty() {
            url.set_password(Some(&urlencoding::encode(password)))
                .map_err(|_| ApiError::bad("密码无效"))?;
        }
        codex2api_storage::parse_proxy_url(url.as_str())?;
        Ok(url.into())
    }
}
pub async fn create(State(s): State<AdminState>, Json(f): Json<Input>) -> ApiResult {
    let url = f.url(f.password.as_deref().unwrap_or(""))?;
    let p = s.storage.create_outbound_proxy(&f.name, &url).await?;
    Ok(Json(dto(&p, 0)))
}
pub async fn update(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<Input>,
) -> ApiResult {
    let old = s.storage.require_outbound_proxy(&id).await?;
    let old_url = codex2api_storage::parse_proxy_url(&old.url)?;
    let password = f.password.clone().unwrap_or_else(|| {
        urlencoding::decode(old_url.password().unwrap_or(""))
            .unwrap_or_default()
            .into_owned()
    });
    let url = f.url(&password)?;
    s.storage.update_outbound_proxy(&id, &f.name, &url).await?;
    for account in s
        .storage
        .list_accounts()
        .await?
        .into_iter()
        .filter(|a| a.proxy_id.as_deref() == Some(&id))
    {
        s.upstream.evict(&account.id).await;
        s.auth.evict_account_http(&account.id).await;
    }
    detail(State(s), Path(id)).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    #[serde(default)]
    confirm_unbind: bool,
}
pub async fn delete(
    State(s): State<AdminState>,
    Path(id): Path<String>,
    Json(f): Json<Delete>,
) -> Result<axum::response::Response, ApiError> {
    use axum::response::IntoResponse;
    let bound = s
        .storage
        .list_accounts()
        .await?
        .into_iter()
        .filter(|a| a.proxy_id.as_deref() == Some(&id))
        .map(|a| a.id)
        .collect::<Vec<_>>();
    match s.storage.delete_outbound_proxy(&id,f.confirm_unbind).await {
        Ok(true) => {
            for account in bound {
                s.upstream.evict(&account).await;
                s.auth.evict_account_http(&account).await;
            }
            Ok(ok().into_response())
        }
        Ok(false) => Err(ApiError::missing()),
        Err(codex2api_storage::StorageError::ProxyHasBindings(count)) => Ok((
            axum::http::StatusCode::CONFLICT,
            Json(json!({"error":{"code":"proxy_has_bindings","message":"请确认解除供应账户绑定后删除"},"confirmation_required":true,"account_count":count})),
        ).into_response()),
        Err(e) => Err(e.into()),
    }
}
pub async fn check(
    State(s): State<AdminState>,
    Path((id, action)): Path<(String, String)>,
) -> ApiResult {
    if !matches!(action.as_str(), "test" | "quality" | "timezone") {
        return Err(ApiError::missing());
    }
    let p = s.storage.require_outbound_proxy(&id).await?;
    let p = if action == "quality" {
        s.storage
            .save_proxy_quality_check(&p, &proxy_checks::quality(&p).await)
            .await?
    } else {
        s.storage
            .save_proxy_connection_check(&p, &proxy_checks::connection(&p).await)
            .await?
    };
    if action == "timezone" {
        return match p.timezone {
            Some(t) if p.connection_ok == Some(true) => Ok(Json(json!({"timezone":t}))),
            _ => Err(ApiError::upstream("未获得有效的代理出口时区")),
        };
    }
    let counts = s.storage.outbound_proxy_account_counts().await?;
    Ok(Json(dto(&p, counts.get(&id).copied().unwrap_or(0))))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_credentials_round_trip() {
        for protocol in ["http", "https", "socks5", "socks5h"] {
            let mut f = Input {
                name: "test".into(),
                protocol: protocol.into(),
                host: "::1".into(),
                port: 8080,
                username: "user%@:/".into(),
                password: None,
            };
            let p = "p%20@?#密碼";
            let u = url::Url::parse(&f.url(p).ok().unwrap()).unwrap();
            assert_eq!(u.host_str(), Some("[::1]"));
            assert_eq!(urlencoding::decode(u.username()).unwrap(), f.username);
            assert_eq!(urlencoding::decode(u.password().unwrap()).unwrap(), p);
            for host in ["http://host", "host:8080", "host/path", "user@host"] {
                f.host = host.into();
                assert!(f.url(p).is_err());
            }
        }
    }
}

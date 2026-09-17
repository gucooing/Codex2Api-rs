//! Calls to official account services shared by administrator pages.
use crate::AdminState;
use axum::http::HeaderMap;
use bytes::Bytes;
use codex2api_upstream::BackendEndpoint as E;
use serde_json::Value;
use std::collections::HashMap;

pub(crate) async fn quota(
    state: &AdminState,
    id: &str,
    refresh: bool,
) -> Result<codex2api_storage::QuotaSnapshot, String> {
    state
        .quota_cache
        .get_or_fetch(id, refresh, || async {
            request(state, id, E::Usage, &HashMap::new(), None, None).await
        })
        .await
}

pub(crate) async fn request(
    state: &AdminState,
    id: &str,
    endpoint: E,
    params: &HashMap<String, String>,
    query: Option<&str>,
    value: Option<Value>,
) -> Result<Value, String> {
    let future = async {
        let account = state
            .storage
            .require_account(id)
            .await
            .map_err(|e| e.to_string())?;
        if account.status == codex2api_storage::AccountStatus::Pending {
            return Err("账户尚未完成授权".into());
        }
        let client = state.upstream.get(id).await.map_err(|e| e.to_string())?;
        let body = value
            .map(|v| serde_json::to_vec(&v))
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let response = client
            .forward_backend(endpoint, params, query, Bytes::from(body), HeaderMap::new())
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| format!("官方返回 HTTP {}，响应不是 JSON", status.as_u16()))?;
        if !status.is_success() {
            let message = value
                .pointer("/error/message")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("请求未成功");
            return Err(format!(
                "官方返回 HTTP {}：{}",
                status.as_u16(),
                message.chars().take(500).collect::<String>()
            ));
        }
        Ok(value)
    };
    tokio::time::timeout(std::time::Duration::from_secs(30), future)
        .await
        .map_err(|_| {
            if endpoint == E::ConsumeCredit {
                "官方响应超时，操作结果尚未确认，请先查看额度状态".to_string()
            } else {
                "官方请求超时，请重试".to_string()
            }
        })?
}

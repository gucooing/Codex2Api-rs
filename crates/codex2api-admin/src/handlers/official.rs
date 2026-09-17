use crate::models::{PageQuery, Section};
use crate::services::request;
use crate::views::{self as html, official as official_html};
use crate::{AdminState, session};
use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use codex2api_upstream::BackendEndpoint as E;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub(crate) fn csrf_token(session_id: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("codex2api-official-actions:{session_id}"))
    )
}

/// Compatibility URLs keep working; all account tabs now share the detail page.
pub async fn page(Path(id): Path<String>, Query(query): Query<PageQuery>) -> Response {
    let tab = match query.tab.as_str() {
        "account" => "details",
        "usage" => "usage",
        "credits" => "credits",
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("tab", tab);
    if let Some(ok) = query.ok {
        params.append_pair("ok", &ok);
    }
    if let Some(err) = query.err {
        params.append_pair("err", &err);
    }
    Redirect::to(&format!("/admin/accounts/{id}?{}", params.finish())).into_response()
}

pub(crate) async fn account_content(
    state: &AdminState,
    account: &codex2api_storage::Account,
    tab: &str,
    csrf: &str,
) -> String {
    let (endpoint, title) = match tab {
        "usage" => (E::Profile, "用量明细（官方数据）"),
        "details" => (E::Accounts, "官方账户信息"),
        "credits" => (E::Credits, "额度重置"),
        _ => unreachable!("validated account tab"),
    };
    let result = request(state, &account.id, endpoint, &HashMap::new(), None, None).await;
    let sections = [Section {
        title,
        endpoint,
        result,
    }];
    let content = official_html::sections(account, &sections, csrf);
    if tab == "details" {
        html::account::details(account, &content)
    } else {
        content
    }
}

#[derive(Deserialize)]
pub struct ActionForm {
    csrf: String,
    action: String,
    #[serde(default)]
    credit_id: String,
    #[serde(default)]
    redeem_request_id: String,
}

fn action_payload(form: &ActionForm) -> Result<(E, Value), String> {
    match form.action.as_str() {
        "consume" => {
            uuid::Uuid::parse_str(&form.redeem_request_id)
                .map_err(|_| "无效的重置请求，请刷新页面".to_string())?;
            let mut payload = json!({"redeem_request_id":form.redeem_request_id});
            if !form.credit_id.is_empty() {
                payload["credit_id"] = form.credit_id.clone().into();
            }
            Ok((E::ConsumeCredit, payload))
        }
        _ => Err("不支持的操作".into()),
    }
}

pub async fn action(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<ActionForm>,
) -> Response {
    let Some(session) = session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    if form.csrf != csrf_token(&session.id) {
        return (
            StatusCode::FORBIDDEN,
            Html(html::error_page("请求无效", "请刷新页面后重试")),
        )
            .into_response();
    }
    if form.action != "consume" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let result = match action_payload(&form) {
        Ok((endpoint, payload)) => {
            request(&state, &id, endpoint, &HashMap::new(), None, Some(payload)).await
        }
        Err(error) => Err(error),
    };
    if result.is_ok() {
        state.quota_cache.invalidate(&id).await;
    }
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("tab", "credits");
    match result {
        Ok(value) => {
            let message = match value.get("code").and_then(Value::as_str) {
                Some("reset") => format!(
                    "已重置 {} 个额度窗口",
                    value
                        .get("windows_reset")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                ),
                Some("nothing_to_reset") => "当前没有需要重置的额度窗口".into(),
                Some("no_credit") => "没有可用的重置额度".into(),
                Some("already_redeemed") => "此请求已经处理，无需重复使用".into(),
                Some(other) => format!("官方返回：{other}"),
                None => "官方已返回响应，但未提供重置结果".into(),
            };
            query.append_pair("ok", &message);
        }
        Err(error) => {
            query.append_pair("err", &error);
        }
    }
    Redirect::to(&format!("/admin/accounts/{id}?{}", query.finish())).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reset_is_idempotent_and_removed_actions_are_rejected() {
        let mut form = ActionForm {
            csrf: "unused".into(),
            action: "consume".into(),
            credit_id: String::new(),
            redeem_request_id: uuid::Uuid::new_v4().to_string(),
        };
        assert_eq!(
            action_payload(&form).unwrap().1,
            action_payload(&form).unwrap().1
        );
        assert_ne!(csrf_token("session-a"), csrf_token("session-b"));
        form.action = "create_task".into();
        assert!(action_payload(&form).is_err());
    }
}

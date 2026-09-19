use crate::AdminState;
use crate::response::{encode_query, not_found_account, storage_error};
use crate::views as html;
use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use codex2api_storage::{AccountStatus, StorageError};
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct DashboardQuery {
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub plan: String,
    pub ok: Option<String>,
    pub err: Option<String>,
}

#[derive(Deserialize)]
pub struct AccountQuery {
    #[serde(default = "default_account_tab")]
    pub tab: String,
    pub ok: Option<String>,
    pub err: Option<String>,
}
fn default_account_tab() -> String {
    "info".into()
}

pub async fn dashboard(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Query(query): Query<DashboardQuery>,
) -> Response {
    let Some(session) = crate::session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    match state.storage.list_accounts().await {
        Ok(mut accounts) => {
            let plans = accounts
                .iter()
                .filter_map(|account| account.plan_type.clone())
                .filter(|plan| !plan.is_empty())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let keyword = query.account.trim().to_lowercase();
            accounts.retain(|account| {
                let status = match account.status {
                    AccountStatus::Active => "active",
                    AccountStatus::Disabled => "disabled",
                    AccountStatus::Pending => "pending",
                };
                (query.status.is_empty() || query.status == status)
                    && (query.plan.is_empty()
                        || account.plan_type.as_deref() == Some(query.plan.as_str()))
                    && (keyword.is_empty()
                        || [account.display_name.as_deref(), account.email.as_deref()]
                            .into_iter()
                            .flatten()
                            .any(|value| value.to_lowercase().contains(&keyword)))
            });
            let mut plan_expirations = std::collections::HashMap::new();
            for account in &accounts {
                let tokens = match state.storage.load_account_tokens(&account.id).await {
                    Ok(tokens) => tokens,
                    Err(error) => return storage_error("无法加载套餐有效期", error),
                };
                if let Some(expiration) = tokens
                    .and_then(|tokens| tokens.id_token)
                    .as_deref()
                    .and_then(|token| {
                        codex2api_auth::parse_chatgpt_subscription_expiration(token).ok()
                    })
                    .flatten()
                {
                    plan_expirations.insert(account.id.clone(), expiration);
                }
            }
            (
                [(axum::http::header::CACHE_CONTROL, "no-store")],
                Html(html::dashboard(
                    &accounts,
                    &query,
                    &plans,
                    &plan_expirations,
                    state.last_oauth().as_ref(),
                    &super::official::csrf_token(&session.id),
                )),
            )
                .into_response()
        }
        Err(err) => storage_error("无法加载账户列表", err),
    }
}

#[derive(Default, Deserialize)]
pub struct QuotaQuery {
    #[serde(default)]
    pub refresh: bool,
}

fn quota_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        axum::Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

pub async fn account_quota(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    Query(query): Query<QuotaQuery>,
) -> Response {
    match state.storage.get_account(&id).await {
        Ok(Some(_)) => {}
        Ok(None) => return quota_error(StatusCode::NOT_FOUND, "账户不存在"),
        Err(error) => {
            return quota_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("无法加载账户：{error}"),
            );
        }
    }
    let snapshot = match crate::services::quota(&state, &id, query.refresh).await {
        Ok(snapshot) => snapshot,
        Err(error) => return quota_error(StatusCode::BAD_GATEWAY, &error),
    };
    let quota = Ok(snapshot.value);
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Html(html::usage::quota_inline(
            Some(&quota),
            snapshot.observed_at,
            chrono::Utc::now(),
        )),
    )
        .into_response()
}

pub async fn account_page(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<AccountQuery>,
) -> Response {
    if !matches!(
        query.tab.as_str(),
        "info" | "fingerprint" | "usage" | "details" | "credits" | "turn-state"
    ) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let account = match state.storage.get_account(&id).await {
        Ok(Some(account)) => account,
        Ok(None) => return not_found_account(&id),
        Err(error) => return storage_error("无法加载账户", error),
    };
    let content = match query.tab.as_str() {
        "turn-state" => {
            let Some(session) = crate::session::load_session(&state.storage, &headers).await else {
                return Redirect::to("/admin/login").into_response();
            };
            match super::turn_state::content(
                &state,
                &account,
                &super::official::csrf_token(&session.id),
            )
            .await
            {
                Ok(content) => content,
                Err(error) => return storage_error("无法加载状态复用配置", error),
            }
        }
        "fingerprint" => {
            let Some(session) = crate::session::load_session(&state.storage, &headers).await else {
                return Redirect::to("/admin/login").into_response();
            };
            let form = super::fingerprint::FingerprintForm::from_account(
                &account,
                &super::official::csrf_token(&session.id),
            );
            let proxies = match state.storage.list_outbound_proxies().await {
                Ok(proxies) => proxies,
                Err(error) => return storage_error("无法加载代理配置", error),
            };
            html::fingerprint::render(&account, &form, &proxies)
        }
        "info" => {
            let Some(session) = crate::session::load_session(&state.storage, &headers).await else {
                return Redirect::to("/admin/login").into_response();
            };
            let quota = crate::services::quota(&state, &id, false).await;
            let now = chrono::Utc::now();
            let observed_at = quota.as_ref().map_or(now, |snapshot| snapshot.observed_at);
            let quota = quota.map(|snapshot| snapshot.value);
            let quota = html::usage::quota_panel(&id, &quota, observed_at, now);
            html::account::info(&account, &quota, &super::official::csrf_token(&session.id))
        }
        _ => {
            let Some(session) = crate::session::load_session(&state.storage, &headers).await else {
                return Redirect::to("/admin/login").into_response();
            };
            super::official::account_content(
                &state,
                &account,
                &query.tab,
                &super::official::csrf_token(&session.id),
            )
            .await
        }
    };
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Html(html::account::render(
            &account,
            &query.tab,
            query.ok.as_deref(),
            query.err.as_deref(),
            &content,
        )),
    )
        .into_response()
}

pub async fn enable_account(State(state): State<AdminState>, Path(id): Path<String>) -> Response {
    match state.storage.get_account(&id).await {
        Ok(Some(account)) if account.status == AccountStatus::Pending => Redirect::to(&format!(
            "/admin/accounts/{id}?err={}",
            encode_query("待授权账户需要先完成 OAuth")
        ))
        .into_response(),
        Ok(Some(_)) => match state
            .storage
            .set_account_status(&id, AccountStatus::Active)
            .await
        {
            Ok(_) => Redirect::to(&format!(
                "/admin/accounts/{id}?ok={}",
                encode_query("账户已启用")
            ))
            .into_response(),
            Err(err) => storage_error("无法启用账户", err),
        },
        Ok(None) => not_found_account(&id),
        Err(err) => storage_error("无法启用账户", err),
    }
}

pub async fn disable_account(State(state): State<AdminState>, Path(id): Path<String>) -> Response {
    match state
        .storage
        .set_account_status(&id, AccountStatus::Disabled)
        .await
    {
        Ok(_) => Redirect::to(&format!(
            "/admin/accounts/{id}?ok={}",
            encode_query("账户已停用")
        ))
        .into_response(),
        Err(StorageError::AccountNotFound(_)) => not_found_account(&id),
        Err(err) => storage_error("无法停用账户", err),
    }
}

#[derive(Deserialize)]
pub struct DeleteForm {
    csrf: String,
}

pub async fn delete_account(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<DeleteForm>,
) -> Response {
    let Some(session) = crate::session::load_session(&state.storage, &headers).await else {
        return Redirect::to("/admin/login").into_response();
    };
    if form.csrf != super::official::csrf_token(&session.id) {
        return (
            StatusCode::FORBIDDEN,
            Html(html::error_page("请求无效", "请刷新页面后重试")),
        )
            .into_response();
    }
    match state.storage.delete_account(&id).await {
        Ok(true) => {
            state.upstream.evict(&id).await;
            state.auth.evict_account_http(&id).await;
            state.quota_cache.evict(&id).await;
            state.clear_account_oauth(&id);
            Redirect::to(&format!("/admin?ok={}", encode_query("账户已删除"))).into_response()
        }
        Ok(false) => not_found_account(&id),
        Err(error) => storage_error("无法删除账户", error),
    }
}

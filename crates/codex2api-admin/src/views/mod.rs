//! Shared server-rendered admin pages and HTML escaping.
pub(crate) mod account;
pub(crate) mod fingerprint;
pub(crate) mod keys;
pub(crate) mod oauth;
pub(crate) mod oauth_credentials;
pub(crate) mod official;
pub(crate) mod proxies;
pub(crate) mod settings;
pub(crate) mod usage;
pub(crate) mod usage_records;

use crate::state::InflightOauth;
use codex2api_storage::{Account, AccountStatus};
use codex2api_version::CODEX_REF_COMMIT;

const CSS: &str = include_str!("../../static/app.css");
const COPY_SCRIPT: &str = include_str!("../../static/copy.js");
const NAV_SCRIPT: &str = include_str!("../../static/nav.js");

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn opt(value: Option<&str>) -> String {
    match value.map(str::trim).filter(|s| !s.is_empty()) {
        Some(v) => esc(v),
        None => "—".to_string(),
    }
}

pub(crate) fn page(title: &str, nav: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title}</title>
  <style>{CSS}</style>
</head>
<body>
  {nav}
  <div class="wrap">
    {body}
    <footer>Codex2API · 官方快照 {commit}</footer>
  </div>
  <script>{COPY_SCRIPT}{NAV_SCRIPT}</script>
</body>
</html>"#,
        title = esc(title),
        commit = esc(&CODEX_REF_COMMIT[..12.min(CODEX_REF_COMMIT.len())]),
    )
}

pub(crate) fn nav(current: &str) -> String {
    let oauth_active = if current == "oauth-credentials" {
        " active"
    } else {
        ""
    };
    let settings_active = if current == "settings" { " active" } else { "" };
    let (account_active, usage_active, keys_active, proxies_active) = match current {
        "usage-records" => ("", " active", "", ""),
        "keys" => ("", "", " active", ""),
        "proxies" => ("", "", "", " active"),
        "settings" | "oauth-credentials" => ("", "", "", ""),
        _ => (" active", "", "", ""),
    };
    format!(
        r#"<header class="top">
  <div class="top-inner">
    <h1>Codex2API 管理</h1>
    <nav class="main-tabs" aria-label="管理导航">
      <a class="main-tab{account_active}" href="/admin">账户管理</a>
      <a class="main-tab{usage_active}" href="/admin/usage">用量管理</a>
      <a class="main-tab{keys_active}" href="/admin/keys">API Key</a>
      <a class="main-tab{oauth_active}" href="/admin/oauth/credentials">OAuth</a>
      <a class="main-tab{proxies_active}" href="/admin/proxies">代理配置</a>
    </nav>
    <a class="main-tab top-settings{settings_active}" href="/admin/settings">设置</a>
    <details class="top-menu" data-admin-menu>
      <summary class="btn ghost" aria-label="管理菜单" aria-expanded="false" aria-controls="admin-menu-options">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></svg>
      </summary>
      <div class="top-menu-options" id="admin-menu-options">
        <form method="post" action="/admin/logout"><button type="submit">退出</button></form>
      </div>
    </details>
  </div>
</header>"#
    )
}

pub fn login(error: Option<&str>, notice: Option<&str>) -> String {
    let messages = flash(notice, error);
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Codex2API 登录</title>
  <style>{CSS}</style>
</head>
<body>
  <div class="login-wrap">
    <div class="card">
      <h1>Codex2API</h1>
      <p class="hint">管理端只有一个管理员。首次启动默认账号 admin / admin。</p>
      {messages}
      <form class="stack" method="post" action="/admin/login">
        <label>用户名 <input name="username" value="admin" autocomplete="username" autofocus></label>
        <label>密码 <input name="password" type="password" autocomplete="current-password"></label>
        <button type="submit">登录</button>
      </form>
    </div>
  </div>
</body>
</html>"#
    )
}

pub fn flash(ok: Option<&str>, err: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(msg) = ok.filter(|s| !s.is_empty()) {
        out.push_str(&format!(r#"<div class="flash ok">{}</div>"#, esc(msg)));
    }
    if let Some(msg) = err.filter(|s| !s.is_empty()) {
        out.push_str(&format!(r#"<div class="flash err">{}</div>"#, esc(msg)));
    }
    out
}

fn status_badge(status: AccountStatus) -> String {
    let (cls, label) = match status {
        AccountStatus::Active => ("active", "已启用"),
        AccountStatus::Disabled => ("disabled", "已停用"),
        AccountStatus::Pending => ("pending", "待授权"),
    };
    format!(r#"<span class="badge {cls}">{label}</span>"#)
}

fn account_datetime(value: Option<chrono::DateTime<chrono::Utc>>) -> String {
    value.map_or_else(
        || "—".to_string(),
        |date| {
            format!(
                r#"<time data-account-time datetime="{}">{} UTC</time>"#,
                date.to_rfc3339(),
                date.format("%Y-%m-%d %H:%M:%S")
            )
        },
    )
}

pub fn dashboard(
    accounts: &[Account],
    query: &crate::handlers::accounts::DashboardQuery,
    plans: &[String],
    plan_expirations: &std::collections::HashMap<String, chrono::DateTime<chrono::Utc>>,
    inflight: Option<&InflightOauth>,
    csrf: &str,
) -> String {
    let inflight_banner = inflight
        .map(|info| {
            format!(
                r#"<div class="flash ok">账户 <span class="mono">{}</span> 正在授权。<a href="/admin/oauth?state={}">继续授权</a></div>"#,
                esc(&info.account_id),
                esc(&info.state)
            )
        })
        .unwrap_or_default();

    let mut status_options = String::from(r#"<option value="">全部状态</option>"#);
    for (value, label) in [
        ("active", "已启用"),
        ("disabled", "已停用"),
        ("pending", "待授权"),
    ] {
        status_options.push_str(&format!(
            r#"<option value="{value}" {}>{label}</option>"#,
            if query.status == value {
                "selected"
            } else {
                ""
            }
        ));
    }
    let mut plan_options = String::from(r#"<option value="">全部套餐</option>"#);
    for plan in plans {
        plan_options.push_str(&format!(
            r#"<option value="{}" {}>{}</option>"#,
            esc(plan),
            if query.plan == *plan { "selected" } else { "" },
            esc(plan)
        ));
    }
    let mut rows = String::from(
        r#"<div class="table-wrap"><table>
<thead><tr>
  <th>状态</th><th>邮箱</th><th>套餐</th><th>套餐有效期</th><th>套餐额度（剩余）</th><th>上次使用时间</th><th>操作</th>
</tr></thead><tbody>"#,
    );
    if accounts.is_empty() {
        rows.push_str(r#"<tr><td colspan="7" class="empty">暂无符合条件的账户</td></tr>"#);
    }
    let now = chrono::Utc::now();
    let quota_placeholder = usage::quota_inline(None, now, now);
    for a in accounts {
        rows.push_str(&format!(
            r#"<tr>
  <td>{}</td>
  <td>{}</td>
  <td>{}</td>
  <td class="account-time">{}</td>
  <td data-account-quota="/admin/accounts/{id}/quota" aria-busy="true"><span class="account-quota-cell"><span data-quota-content>{quota_placeholder}</span><button class="quota-refresh" type="button" data-quota-refresh title="刷新套餐额度" aria-label="刷新套餐额度" disabled><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 7v5h-5M4 17v-5h5"/><path d="M6.1 7a7 7 0 0 1 11.5-1L20 9M4 15l2.4 3a7 7 0 0 0 11.5-1"/></svg></button></span></td>
  <td class="account-time">{}</td>
  <td><div class="key-row-actions"><a class="btn secondary" href="/admin/accounts/{id}">详情</a><form method="post" action="/admin/accounts/{id}/delete" onsubmit="return confirm('删除账户后，其 API Key 将立即失效。确定删除？');"><input type="hidden" name="csrf" value="{csrf}"><button class="danger" type="submit">删除</button></form></div></td>
</tr>"#,
            status_badge(a.status),
            opt(a.email.as_deref()),
            opt(a.plan_type.as_deref()),
            account_datetime(plan_expirations.get(&a.id).copied()),
            account_datetime(a.last_used_at.as_deref().and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok()).map(|date| date.with_timezone(&chrono::Utc))),
            id = esc(&a.id),
            csrf = esc(csrf),
        ));
    }
    rows.push_str("</tbody></table></div>");

    let body = format!(
        r#"{flash}
{inflight}
<section class="card account-toolbar">
  <form class="record-filters account-filters" method="get" action="/admin">
    <label>账户名 / 邮箱<input name="account" value="{account_filter}"></label>
    <label>状态<select name="status">{status_options}</select></label>
    <label>套餐<select name="plan">{plan_options}</select></label>
    <div class="record-filter-actions"><button type="submit">查询</button><a class="btn secondary" href="/admin">重置</a></div>
  </form>
  <div class="record-filter-actions account-add"><button type="button" data-account-wizard-open>添加新账户</button><span class="flash err account-wizard-error" data-account-wizard-error role="alert" hidden></span></div>
</section>
<section class="card account-records">
  {rows}
</section>
<dialog class="key-dialog oauth-wizard" id="account-wizard-dialog" aria-labelledby="account-wizard-title"><div data-account-wizard-content></div></dialog>
<script>{usage_script}{fingerprint_script}{script}</script>"#,
        flash = flash(query.ok.as_deref(), query.err.as_deref()),
        inflight = inflight_banner,
        account_filter = esc(&query.account),
        fingerprint_script = include_str!("../../static/fingerprint.js"),
        usage_script = include_str!("../../static/usage.js"),
        script = include_str!("../../static/accounts.js"),
    );
    page("账户管理", &nav("dash"), &body)
}

pub fn error_page(title: &str, message: &str) -> String {
    let body = format!(
        r#"<div class="card">
  <h2>{}</h2>
  <p class="flash err">{}</p>
  <p><a href="/admin">返回</a></p>
</div>"#,
        esc(title),
        esc(message)
    );
    page(title, &nav("error"), &body)
}

use super::{esc, flash, nav, opt, page};
use codex2api_storage::{
    Account, AccountStatus, OAuthAccountSummary, OAuthCredential, OAuthDevice,
};

const SCRIPT: &str = include_str!("../../static/oauth-credentials.js");

fn label(account: &Account) -> &str {
    account
        .email
        .as_deref()
        .or(account.display_name.as_deref())
        .unwrap_or(&account.id)
}

fn time(value: Option<&str>) -> String {
    value
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|date| {
            format!(
                r#"<time data-oauth-time datetime="{}">{} UTC</time>"#,
                esc(&date.to_rfc3339()),
                date.with_timezone(&chrono::Utc).format("%Y-%m-%d %H:%M:%S")
            )
        })
        .unwrap_or_else(|| "—".into())
}

fn can_add(account: &Account) -> bool {
    account.status == AccountStatus::Active
        && account
            .chatgpt_account_id
            .as_deref()
            .is_some_and(|s| !s.is_empty())
}

fn add_button(enabled: bool) -> &'static str {
    if enabled {
        r#"<button type="button" data-oauth-credential-add>添加 RT</button>"#
    } else {
        r#"<button type="button" data-oauth-credential-add disabled title="请先启用并授权账户">添加 RT</button>"#
    }
}

fn dialog(accounts: &[Account], bound: Option<&Account>, csrf: &str) -> String {
    let account_field = if let Some(account) = bound {
        format!(
            r#"<input type="hidden" name="account_id" value="{}"><p class="hint">绑定账户：{}</p>"#,
            esc(&account.id),
            esc(label(account))
        )
    } else {
        let mut options = String::from(r#"<option value="" disabled selected>请选择账户</option>"#);
        for account in accounts.iter().filter(|a| can_add(a)) {
            options.push_str(&format!(
                r#"<option value="{}">{}</option>"#,
                esc(&account.id),
                esc(label(account))
            ));
        }
        format!(r#"<label>绑定账户<select name="account_id" required>{options}</select></label>"#)
    };
    format!(
        r#"<dialog class="key-dialog" id="oauth-credential-dialog" aria-labelledby="oauth-credential-title">
<form class="stack" method="post" action="/admin/oauth/credentials">
<h2 id="oauth-credential-title">添加 RT</h2><input type="hidden" name="csrf" value="{csrf}">
<label>名称<input name="name" maxlength="128" required autofocus autocomplete="off"></label>
{account_field}
<div class="row-actions"><button class="secondary" type="button" data-oauth-credential-cancel>取消</button><button type="submit">确认添加</button></div>
</form></dialog>"#,
        csrf = esc(csrf)
    )
}

pub(crate) fn render(
    accounts: &[Account],
    summaries: &[OAuthAccountSummary],
    csrf: &str,
    ok: Option<&str>,
    err: Option<&str>,
) -> String {
    let mut rows = String::new();
    for summary in summaries {
        let Some(account) = accounts.iter().find(|a| a.id == summary.account_id) else {
            continue;
        };
        rows.push_str(&format!(r#"<tr><td>{name}</td><td>{status}</td><td>{rts}</td><td>{devices}</td><td>{used}</td><td><div class="key-row-actions"><a class="btn secondary" href="/admin/oauth/accounts/{id}">详情</a><form method="post" action="/admin/oauth/accounts/{id}/delete" onsubmit="return confirm('删除该账户的全部 OAuth RT、令牌和设备记录？原始账户将保留。');"><input type="hidden" name="csrf" value="{csrf}"><button class="danger" type="submit">删除</button></form></div></td></tr>"#,
            name=esc(label(account)),status=super::status_badge(account.status),rts=summary.rt_count,devices=summary.device_count,used=time(summary.last_used_at.as_deref()),id=esc(&account.id),csrf=esc(csrf)));
    }
    if rows.is_empty() {
        rows.push_str(
            r#"<tr><td colspan="6" class="empty">还没有配置 OAuth 的账户，请添加 RT。</td></tr>"#,
        );
    }
    page(
        "OAuth",
        &nav("oauth-credentials"),
        &format!(
            r#"{flash}
<section class="card"><h2>第三方客户端接入</h2>
<p class="hint">服务基础地址使用当前服务地址加 <code>/api/oauth/chatgpt</code>，导入 RT 后即可登录绑定账户。一个账户可添加多个 RT。</p>
<dl class="dl"><dt>Token 刷新</dt><dd class="mono">/api/oauth/chatgpt/oauth/token</dd><dt>Responses（HTTP / WS）</dt><dd class="mono">/api/oauth/chatgpt/backend-api/codex/responses</dd></dl>
</section>
<section class="card"><div class="key-heading"><h2>OAuth 账户</h2>{add}</div>
<div class="table-wrap"><table><thead><tr><th>账户</th><th>账户状态</th><th>RT 数量</th><th>登录设备数</th><th>最近使用</th><th>操作</th></tr></thead><tbody>{rows}</tbody></table></div>
</section>{dialog}<script>{SCRIPT}</script>"#,
            flash = flash(ok, err),
            add = add_button(accounts.iter().any(can_add)),
            dialog = dialog(accounts, None, csrf)
        ),
    )
}

pub(crate) fn detail(
    account: &Account,
    credentials: &[OAuthCredential],
    devices: &[OAuthDevice],
    csrf: &str,
    ok: Option<&str>,
    err: Option<&str>,
) -> String {
    let mut rows = String::new();
    for credential in credentials {
        let (status, class, action, action_label) = if credential.paused_at.is_some() {
            ("已暂停", "pending", "enable", "启用")
        } else {
            ("已启用", "active", "pause", "暂停")
        };
        rows.push_str(&format!(r#"<tr>
<td class="mono">{prefix}</td><td>{name}</td><td>{created}</td><td>{used}</td><td>{devices}</td><td><span class="badge {class}">{status}</span></td>
<td><div class="key-row-actions"><button class="secondary" type="button" data-key-copy="/admin/oauth/credentials/{id}/copy" title="复制 RT">复制 RT</button>
<form method="post" action="/admin/oauth/credentials/{id}/{action}"><input type="hidden" name="csrf" value="{csrf}"><button class="secondary" type="submit">{action_label}</button></form>
<form method="post" action="/admin/oauth/credentials/{id}/delete" onsubmit="return confirm('删除后该 RT 及其 Access Token 将失效，确定删除？');"><input type="hidden" name="csrf" value="{csrf}"><button class="danger" type="submit">删除</button></form>
</div></td></tr>"#,
            id=esc(&credential.id),prefix=esc(&credential.token_prefix),name=esc(&credential.name),created=time(Some(&credential.created_at)),used=time(credential.last_used_at.as_deref()),
            devices=devices.iter().filter(|d| d.credential_id==credential.id).count(),csrf=esc(csrf)));
    }
    if rows.is_empty() {
        rows.push_str(r#"<tr><td colspan="7" class="empty">该账户还没有 RT。</td></tr>"#);
    }
    let mut device_rows = String::new();
    for device in devices {
        device_rows.push_str(&format!(r#"<tr><td>{name}</td><td class="mono">{installation}</td><td class="oauth-device-ua">{ua}</td><td>{first}</td><td>{login}</td><td>{used}</td></tr>"#,
            name=esc(&device.credential_name),installation=opt(device.installation_id.as_deref()),ua=if device.user_agent.is_empty(){"未提供".into()}else{esc(&device.user_agent)},
            first=time(Some(&device.first_login_at)),login=time(Some(&device.last_login_at)),used=time(device.last_used_at.as_deref())));
    }
    if device_rows.is_empty() {
        device_rows.push_str(r#"<tr><td colspan="6" class="empty">暂无登录设备记录。</td></tr>"#);
    }
    page(
        "OAuth 详情",
        &nav("oauth-credentials"),
        &format!(
            r#"{flash}
<section class="card"><div class="key-heading"><h2>{account} · OAuth 详情</h2><a class="btn secondary" href="/admin/oauth/credentials">返回</a></div></section>
<section class="card key-records" data-key-manager data-key-csrf="{csrf}">
<div class="key-heading"><h2>RT 管理</h2>{add}</div>
<p class="hint">每个 RT 固定绑定当前账户。RT 持续有效，直到暂停或删除；Access Token 有效期为 1 小时。</p>
<div class="table-wrap"><table><thead><tr><th>RT 前缀</th><th>名称</th><th>创建</th><th>最近使用</th><th>登录设备数</th><th>状态</th><th>操作</th></tr></thead><tbody>{rows}</tbody></table></div>
</section><section class="card"><h2>登录设备记录</h2>
<p class="hint">按登录请求提供的安装标识识别设备；未提供时按 UA 归类，同 UA 的设备可能合并。最近登录表示成功刷新令牌，最近使用表示通过鉴权的接口请求或 WebSocket 业务消息，不代表设备当前在线。</p>
<div class="table-wrap"><table><thead><tr><th>RT 名称</th><th>设备安装标识</th><th>客户端 / User-Agent</th><th>首次登录</th><th>最近登录</th><th>最近使用</th></tr></thead><tbody>{device_rows}</tbody></table></div>
</section>{dialog}<script>{SCRIPT}</script>"#,
            flash = flash(ok, err),
            account = esc(label(account)),
            csrf = esc(csrf),
            add = add_button(can_add(account)),
            dialog = dialog(&[], Some(account), csrf)
        ),
    )
}

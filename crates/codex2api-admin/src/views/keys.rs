use super::{esc, flash, nav, opt, page};
use codex2api_storage::{Account, AccountStatus, ProxyApiKey};

fn account_label(account: &Account) -> String {
    let name = account
        .display_name
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let email = account
        .email
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    match (name, email) {
        (Some(name), Some(email)) if name != email => format!("{name} · {email}"),
        (_, Some(email)) => email.to_string(),
        (Some(name), _) => name.to_string(),
        _ => account.id.clone(),
    }
}

pub(crate) fn render(
    accounts: &[Account],
    keys: &[ProxyApiKey],
    csrf: &str,
    ok: Option<&str>,
    err: Option<&str>,
) -> String {
    let mut options = String::from(r#"<option value="" disabled selected>请选择账户</option>"#);
    let mut has_accounts = false;
    for account in accounts
        .iter()
        .filter(|account| account.status != AccountStatus::Pending)
    {
        has_accounts = true;
        options.push_str(&format!(
            r#"<option value="{}">{}{}</option>"#,
            esc(&account.id),
            esc(&account_label(account)),
            if account.status == AccountStatus::Disabled {
                "（已停用）"
            } else {
                ""
            }
        ));
    }
    let accounts_by_id: std::collections::HashMap<_, _> = accounts
        .iter()
        .map(|account| (account.id.as_str(), account))
        .collect();
    let mut rows = String::new();
    for key in keys {
        let (status, class, action, label) = if key.paused_at.is_some() {
            ("已暂停", "pending", "enable", "启用")
        } else {
            ("已启用", "active", "pause", "暂停")
        };
        let account = accounts_by_id.get(key.account_id.as_str());
        let binding = account
            .map(|account| {
                format!(
                    r#"<a href="/admin/accounts/{}">{}</a>"#,
                    esc(&account.id),
                    esc(&account_label(account))
                )
            })
            .unwrap_or_else(|| "—".into());
        rows.push_str(&format!(
            r#"<tr>
<td class="mono">{prefix}</td><td>{name}</td><td>{binding}</td><td>{created}</td><td>{used}</td>
<td><span class="badge {class}">{status}</span></td><td><div class="key-row-actions">
<button class="secondary" type="button" data-key-bind="/admin/keys/{key_id}/bind" data-bound-account="{account_id}" data-key-name="{key_name}" {bind_disabled}>更换账户</button>
<form method="post" action="/admin/keys/{key_id}/{action}"><input type="hidden" name="csrf" value="{csrf}"><button class="secondary" type="submit">{label}</button></form>
<button class="secondary" type="button" data-key-copy="/admin/keys/{key_id}/copy" {copy_disabled} title="{copy_title}">复制</button>
<form method="post" action="/admin/keys/{key_id}/delete" onsubmit="return confirm('删除后该 Key 将立即失效，确定删除？');"><input type="hidden" name="csrf" value="{csrf}"><button class="danger" type="submit">删除</button></form>
</div></td></tr>"#,
            key_id = esc(&key.id), csrf = esc(csrf),
            prefix = esc(&key.key_prefix), name = opt(key.name.as_deref()),
            account_id = esc(&key.account_id), key_name = esc(key.name.as_deref().unwrap_or(&key.key_prefix)),
            created = esc(&key.created_at), used = opt(key.last_used_at.as_deref()),
            copy_disabled = if key.can_copy { "" } else { "disabled" },
            copy_title = if key.can_copy { "复制 API Key" } else { "旧 Key 未保存明文，无法复制" },
            bind_disabled = if has_accounts { "" } else { "disabled" },
        ));
    }
    if rows.is_empty() {
        rows.push_str(r#"<tr><td colspan="7" class="empty">还没有 API Key。</td></tr>"#);
    }
    let body = format!(
        r#"{flash}<section class="card key-records" data-key-manager data-key-csrf="{csrf}">
<div class="key-heading"><h2>API Key</h2><button type="button" data-add-key {add_disabled}>添加新 Key</button></div>
<div class="table-wrap"><table><thead><tr><th>前缀</th><th>名称</th><th>绑定账户</th><th>创建</th><th>最近使用</th><th>状态</th><th>操作</th></tr></thead><tbody>{rows}</tbody></table></div>
<dialog class="key-dialog" id="create-key-dialog" aria-labelledby="create-key-title">
<form class="stack" method="post" action="/admin/keys">
<h2 id="create-key-title">添加新 Key</h2>
<input type="hidden" name="csrf" value="{csrf}">
<label>名称<input name="name" maxlength="128" required autofocus autocomplete="off"></label>
<label>绑定账户<select name="account_id" required>{options}</select></label>
<div class="row-actions"><button class="secondary" type="button" data-key-cancel>取消</button><button type="submit">确认添加</button></div>
</form></dialog>
<dialog class="key-dialog" id="bind-key-dialog" aria-labelledby="bind-key-title">
<form class="stack" method="post">
<h2 id="bind-key-title">更换绑定账户</h2>
<p class="muted" data-binding-key-name></p>
<input type="hidden" name="csrf" value="{csrf}">
<label>绑定账户<select name="account_id" required>{options}</select></label>
<div class="row-actions"><button class="secondary" type="button" data-key-cancel>取消</button><button type="submit">确认更换</button></div>
</form></dialog></section><script>{script}</script>"#,
        csrf = esc(csrf),
        flash = flash(ok, err),
        add_disabled = if has_accounts {
            ""
        } else {
            r#"disabled title="请先添加并授权账户""#
        },
        script = include_str!("../../static/keys.js"),
    );
    page("API Key", &nav("keys"), &body)
}

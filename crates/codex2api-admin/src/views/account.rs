//! Account-detail tab layout and local account/key views.
use super::{esc, opt, status_badge};
use codex2api_storage::{Account, AccountStatus};

pub(crate) fn render(
    account: &Account,
    tab: &str,
    ok: Option<&str>,
    err: Option<&str>,
    content: &str,
) -> String {
    let mut tabs = String::from(
        "<div class=\"card account-tabs-card\"><nav class=\"account-tabs\" aria-label=\"账户详情切换\">",
    );
    for (key, label) in [
        ("info", "账户信息"),
        ("fingerprint", "指纹"),
        ("usage", "用量明细（官方数据）"),
        ("details", "账户详细信息"),
    ] {
        let active = tab == key || (tab == "credits" && key == "info");
        tabs.push_str(&format!(
            r#"<a class="account-tab{}" href="/admin/accounts/{}?tab={key}" {}>{label}</a>"#,
            if active { " active" } else { "" },
            esc(&account.id),
            if active { r#"aria-current="page""# } else { "" }
        ));
    }
    tabs.push_str(r#"</nav><a class="btn secondary account-back" href="/admin" aria-label="返回账户列表">返回</a></div><script>
(() => {
  const tabs = document.querySelector('.account-tabs');
  const active = tabs.querySelector('[aria-current="page"]');
  if (active) tabs.scrollLeft = Math.max(0, active.offsetLeft - (tabs.clientWidth - active.offsetWidth) / 2);
})();
</script>"#);
    let body = format!("{tabs}{}{content}", super::flash(ok, err));
    super::page("账户详情", &super::nav("account"), &body)
}

pub(crate) fn info(account: &Account, quota: &str, csrf: &str) -> String {
    let toggle = match account.status {
        AccountStatus::Active => format!(
            r#"<form class="inline-form" method="post" action="/admin/accounts/{}/disable" onsubmit="return confirm('确定停用该账户？');">
                 <button class="danger" type="submit">停用账户</button>
               </form>"#,
            esc(&account.id)
        ),
        AccountStatus::Disabled => format!(
            r#"<form class="inline-form" method="post" action="/admin/accounts/{}/enable">
                 <button type="submit">启用账户</button>
               </form>"#,
            esc(&account.id)
        ),
        AccountStatus::Pending => {
            r#"<span class="muted">完成 OAuth 后才会变为已启用。</span>"#.to_string()
        }
    };

    format!(
        r#"<section class="card">
  <dl class="dl">
    <dt>状态</dt><dd>{status}</dd>
    <dt>显示名</dt><dd>{name}</dd>
    <dt>邮箱</dt><dd>{email}</dd>
    <dt>套餐</dt><dd>{plan}</dd>
  </dl>
  <div class="row-actions" style="margin-top:1rem">
    {toggle}
    <button class="secondary" type="button" data-oauth-dialog-open>重新登录（OAuth）</button>
  </div>
</section>{quota}{oauth_dialog}"#,
        status = status_badge(account.status),
        name = opt(account.display_name.as_deref()),
        email = opt(account.email.as_deref()),
        plan = opt(account.plan_type.as_deref()),
        oauth_dialog =
            super::oauth::method_dialog(&format!("/admin/accounts/{}/relogin", account.id), csrf),
    )
}

pub(crate) fn details(account: &Account, official: &str) -> String {
    format!(
        r#"<section class="card"><h2>账户详细信息</h2>
<dl class="dl">
  <dt>内部 ID</dt><dd class="mono">{id}</dd>
  <dt>状态</dt><dd>{status}</dd>
  <dt>显示名</dt><dd>{name}</dd>
  <dt>邮箱</dt><dd>{email}</dd>
  <dt>套餐</dt><dd>{plan}</dd>
  <dt>ChatGPT Account ID</dt><dd class="mono">{chatgpt}</dd>
  <dt>ChatGPT User ID</dt><dd class="mono">{user}</dd>
  <dt>Installation ID</dt><dd class="mono">{install}</dd>
  <dt>创建时间</dt><dd>{created}</dd>
  <dt>最近使用</dt><dd>{used}</dd>
</dl></section>{official}"#,
        id = esc(&account.id),
        status = status_badge(account.status),
        name = opt(account.display_name.as_deref()),
        email = opt(account.email.as_deref()),
        plan = opt(account.plan_type.as_deref()),
        chatgpt = opt(account.chatgpt_account_id.as_deref()),
        user = opt(account.chatgpt_user_id.as_deref()),
        install = esc(&account.installation_id),
        created = esc(&account.created_at),
        used = opt(account.last_used_at.as_deref())
    )
}

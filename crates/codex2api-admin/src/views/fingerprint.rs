use super::esc;
use crate::handlers::fingerprint::FingerprintForm;
use codex2api_accounts::AccountIdentity;
use codex2api_storage::{Account, OutboundProxy};

pub(crate) fn fields(form: &FingerprintForm, proxies: &[OutboundProxy]) -> String {
    let mut fields = String::new();
    for (name, label, value, max) in [
        ("os_type", "操作系统", &form.os_type, 128),
        ("os_version", "系统版本", &form.os_version, 128),
        ("arch", "架构", &form.arch, 128),
        ("terminal", "终端标识", &form.terminal, 256),
    ] {
        fields.push_str(&format!(r#"<label>{label}<input name="{name}" value="{}" maxlength="{max}" required autocomplete="off"></label>"#, esc(value)));
    }
    let mut proxy_options = String::from(r#"<option value="">不使用代理</option>"#);
    for proxy in proxies {
        proxy_options.push_str(&format!(
            r#"<option value="{}" {}>{} · {}</option>"#,
            esc(&proxy.id),
            if form.proxy_id == proxy.id {
                "selected"
            } else {
                ""
            },
            esc(&proxy.name),
            esc(&proxy.display_url())
        ));
    }
    let mut timezone_options = String::new();
    for timezone in chrono_tz::TZ_VARIANTS {
        timezone_options.push_str(&format!(
            r#"<option value="{}"></option>"#,
            esc(timezone.name())
        ));
    }
    fields.push_str(&format!(r#"<label>代理<select name="proxy_id">{proxy_options}</select></label>
<div class="fingerprint-timezone"><label for="fingerprint-timezone">时区</label>
<div class="fingerprint-timezone-row"><input id="fingerprint-timezone" name="timezone" value="{}" list="fingerprint-timezone-options" placeholder="Asia/Taipei" autocomplete="off"><button class="secondary" type="button" data-apply-proxy-timezone {}>应用代理时区</button></div>
<datalist id="fingerprint-timezone-options">{timezone_options}</datalist></div>"#,
        esc(&form.timezone), if form.proxy_id.is_empty() { "hidden" } else { "" }));
    fields
}

pub(crate) fn render(
    account: &Account,
    form: &FingerprintForm,
    proxies: &[OutboundProxy],
) -> String {
    let fields = fields(form, proxies);
    let identity = AccountIdentity::from_account(account);
    format!(
        r#"<section class="card">
  <dl class="dl">
    <dt>Originator</dt><dd class="mono">{originator}</dd>
    <dt>Codex 版本</dt><dd>{version}</dd>
    <dt>Installation ID</dt><dd class="mono">{installation}</dd>
    <dt>当前 User-Agent</dt><dd class="mono">{user_agent}</dd>
  </dl>
  <form class="stack fingerprint-form" method="post" action="/admin/accounts/{id}/fingerprint">
    <input type="hidden" name="csrf" value="{csrf}">
    <div class="fingerprint-fields">{fields}</div>
    <p class="flash err" data-proxy-timezone-error role="alert" hidden></p>
    <p class="hint">User-Agent 根据以上配置自动生成，保存后用于该账户后续请求。</p>
    <p class="hint">时区留空时保留请求中的时区；填写后使用该时区及对应的当地日期。</p>
    <div class="row-actions"><button type="submit">保存指纹配置</button></div>
  </form>
</section><script>{script}</script>"#,
        id = esc(&account.id),
        csrf = esc(&form.csrf),
        originator = esc(codex2api_version::DEFAULT_ORIGINATOR),
        version = esc(codex2api_version::CODEX_PACKAGE_VERSION),
        installation = esc(&account.installation_id),
        user_agent = esc(&identity.official_user_agent()),
        script = include_str!("../../static/fingerprint.js"),
    )
}

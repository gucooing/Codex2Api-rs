use super::{esc, flash, nav, page};
use crate::handlers::fingerprint::FingerprintForm;
use codex2api_accounts::AccountIdentity;
use codex2api_auth::LoginFlow;
use codex2api_storage::{OAuthPending, OutboundProxy};

const SCRIPT: &str = include_str!("../../static/oauth.js");

pub(crate) fn method_dialog(action: &str, csrf: &str) -> String {
    format!(
        r#"<dialog class="key-dialog" id="oauth-method-dialog" aria-labelledby="oauth-method-title">
<form class="stack" method="post" action="{}">
<h2 id="oauth-method-title">选择授权方式</h2>
<input type="hidden" name="csrf" value="{}">
<button type="submit" name="method" value="callback">使用回调链接</button>
<button type="submit" name="method" value="device">使用设备码</button>
<button class="secondary" type="button" data-oauth-cancel>取消</button>
</form></dialog><script>{SCRIPT}</script>"#,
        esc(action),
        esc(csrf)
    )
}

pub(crate) fn setup(
    identity: &AccountIdentity,
    form: &FingerprintForm,
    proxies: &[OutboundProxy],
) -> String {
    let fields = super::fingerprint::fields(form, proxies);
    format!(
        r#"<form class="stack fingerprint-form oauth-wizard-form" method="post" action="/admin/oauth/start" data-account-wizard>
<div class="key-heading"><h2 id="account-wizard-title">新增账户</h2><button class="ghost" type="button" data-account-wizard-close aria-label="关闭">×</button></div>
<ol class="oauth-steps" aria-label="新增账户进度">
  <li data-wizard-indicator="1" aria-current="step"><span>1</span>指纹配置</li>
  <li data-wizard-indicator="2"><span>2</span>授权方式</li>
  <li data-wizard-indicator="3"><span>3</span>开始授权</li>
</ol>
<input type="hidden" name="csrf" value="{csrf}">
<input type="hidden" name="installation_id" value="{installation}">
<p class="flash err" data-wizard-error role="alert" hidden></p>
<section class="oauth-step-panel" data-wizard-step="1">
  <dl class="dl oauth-identity-summary">
    <dt>Originator</dt><dd class="mono">{originator}</dd>
    <dt>Codex 版本</dt><dd>{version}</dd>
    <dt>Installation ID</dt><dd class="mono">{installation}</dd>
  </dl>
  <div class="fingerprint-fields">{fields}</div>
  <p class="flash err" data-proxy-timezone-error role="alert" hidden></p>
  <div class="row-actions oauth-wizard-actions"><button class="secondary" type="button" data-account-wizard-close>取消</button><button type="button" data-wizard-next>下一步</button></div>
</section>
<section class="oauth-step-panel" data-wizard-step="2" hidden>
  <div class="oauth-methods">
    <label><input type="radio" name="method" value="callback" required checked><span><strong>回调链接</strong><small>打开授权页面后提交完整回调链接</small></span></label>
    <label><input type="radio" name="method" value="device" required><span><strong>设备码</strong><small>在设备授权页面输入一次性代码</small></span></label>
    <label><input type="radio" name="method" value="refresh_token" required><span><strong>RT 授权</strong><small>使用 Refresh Token 完成授权</small></span></label>
  </div>
  <div class="row-actions oauth-wizard-actions"><button class="secondary" type="button" data-wizard-back>上一步</button><button type="button" data-wizard-next>下一步</button></div>
</section>
<section class="oauth-step-panel" data-wizard-step="3" hidden>
  <dl class="dl oauth-review">
    <dt>授权方式</dt><dd data-review-method></dd>
    <dt>运行环境</dt><dd data-review-runtime></dd>
    <dt>代理</dt><dd data-review-proxy></dd>
    <dt>时区</dt><dd data-review-timezone></dd>
  </dl>
  <label data-refresh-token-field hidden>Refresh Token<textarea name="refresh_token" rows="4" maxlength="65536" autocomplete="off" spellcheck="false" disabled></textarea></label>
  <div class="stack" data-wizard-callback hidden>
    <a data-wizard-authorize target="_blank" rel="noopener noreferrer">打开授权页面</a>
    <div class="urlbox" id="wizard-authorize-url" data-wizard-url></div>
    <button class="secondary" type="button" data-copy-target="wizard-authorize-url">复制授权链接</button>
    <p class="hint">完成授权后，请复制浏览器地址栏中的完整回调链接并提交。即使回调页面无法打开，也可以复制该链接。</p>
    <label>完整回调链接<input type="url" name="callback_url" autocomplete="off" spellcheck="false" placeholder="http://localhost:1455/auth/callback?code=…&amp;state=…" disabled></label>
  </div>
  <div class="stack" data-wizard-device hidden>
    <a data-wizard-verification target="_blank" rel="noopener noreferrer">打开设备授权页面</a>
    <p>在授权页面输入以下设备码，有效期为 15 分钟。</p>
    <div class="urlbox" id="wizard-device-code" data-wizard-code></div>
    <button class="secondary" type="button" data-copy-target="wizard-device-code">复制设备码</button>
    <p data-wizard-device-status role="status">等待完成授权…</p>
    <button class="secondary" type="button" data-wizard-poll-retry hidden>重试查询</button>
  </div>
  <p data-wizard-status role="status" hidden></p>
  <div class="row-actions oauth-wizard-actions"><button class="secondary" type="button" data-wizard-back>上一步</button><button type="submit" data-wizard-submit>开始授权</button></div>
</section>
</form>"#,
        csrf = esc(&form.csrf),
        installation = esc(&identity.installation_id),
        originator = esc(codex2api_version::DEFAULT_ORIGINATOR),
        version = esc(codex2api_version::CODEX_PACKAGE_VERSION),
    )
}

pub(crate) fn pending(
    pending: &OAuthPending,
    flow: &LoginFlow,
    csrf: &str,
    error: Option<&str>,
) -> String {
    let content = match flow {
        LoginFlow::Callback { authorize_url } => format!(
            r#"<section class="card">
<h2>回调链接授权</h2>
<p><a href="{url}" target="_blank" rel="noopener noreferrer">打开授权页面</a></p>
<div class="urlbox" id="authorize-url">{url}</div>
<p><button class="secondary" type="button" data-copy-target="authorize-url">复制授权链接</button></p>
<p class="hint">完成授权后，请复制浏览器地址栏中的完整回调链接。即使回调页面无法打开，也可以复制该链接并在下方提交。</p>
<form class="stack" method="post" action="/admin/oauth/callback" data-callback-form>
<input type="hidden" name="csrf" value="{csrf}"><input type="hidden" name="state" value="{state}">
<label>完整回调链接<input type="url" name="callback_url" required autocomplete="off" spellcheck="false" placeholder="http://localhost:1455/auth/callback?code=…&amp;state=…"></label>
<div class="row-actions"><button type="submit">提交回调链接</button><a class="btn secondary" href="/admin">返回账户列表</a></div>
</form></section>"#,
            url = esc(authorize_url),
            csrf = esc(csrf),
            state = esc(&pending.state)
        ),
        LoginFlow::Device {
            verification_url,
            user_code,
            interval,
            ..
        } => format!(
            r#"<section class="card" data-device-login data-state="{state}" data-csrf="{csrf}" data-interval="{interval}">
<h2>设备码授权</h2>
<p><a href="{url}" target="_blank" rel="noopener noreferrer">打开设备授权页面</a></p>
<p>在授权页面输入以下设备码，有效期为 15 分钟。</p>
<div class="urlbox" id="oauth-device-code">{code}</div>
<p><button class="secondary" type="button" data-copy-target="oauth-device-code">复制设备码</button></p>
<p data-device-status role="status">等待完成授权…</p>
<a class="btn secondary" href="/admin">返回账户列表</a>
</section>"#,
            url = esc(verification_url),
            code = esc(user_code),
            state = esc(&pending.state),
            csrf = esc(csrf)
        ),
    };
    page(
        "账户授权",
        &nav("oauth"),
        &format!("{}{content}<script>{SCRIPT}</script>", flash(None, error)),
    )
}

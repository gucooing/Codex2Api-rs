use super::{esc, flash, nav, page};

fn tabs(security: bool) -> String {
    format!(
        r#"<nav class="settings-tabs main-tabs" aria-label="设置切换">
<a class="main-tab{}" href="/admin/settings" {}>网关设置</a>
<a class="main-tab{}" href="/admin/settings/security" {}>账户安全设置</a>
</nav>"#,
        if security { "" } else { " active" },
        if security {
            ""
        } else {
            r#"aria-current="page""#
        },
        if security { " active" } else { "" },
        if security {
            r#"aria-current="page""#
        } else {
            ""
        },
    )
}

pub(crate) fn gateway(
    csrf: &str,
    settings: &codex2api_storage::GatewaySettings,
    saved: bool,
    error: Option<&str>,
) -> String {
    page(
        "网关设置",
        &nav("settings"),
        &format!(
            r#"{tabs}
<section class="card gateway-settings">
<h2>UA 黑白名单</h2>
{message}
<form class="stack" method="post" action="/admin/settings">
  <input type="hidden" name="csrf" value="{csrf}">
  <p class="hint" id="ua-rules-hint">适用于 Responses、图片、搜索等 API Key 计费接口及其兼容路径和 WebSocket 入口。每行一条，支持多条规则；忽略大小写，按包含匹配，* 可匹配任意字符。</p>
  <label>名单模式<select name="ua_mode"><option value="blacklist" {blacklist_selected}>黑名单模式</option><option value="whitelist" {whitelist_selected}>白名单模式</option></select></label>
  <label>UA 规则<textarea name="ua_rules" rows="8" aria-describedby="ua-rules-hint" placeholder="codex_cli_rs&#10;Example*Client">{rules}</textarea></label>
  <p class="hint">黑名单模式：命中任一规则返回 429，留空则不限制。白名单模式：仅允许命中任一规则的 UA，其余返回 429；留空会拒绝所有计费请求。保存后对新请求立即生效。</p>
  <div class="row-actions"><button type="submit">保存设置</button></div>
</form>
</section>"#,
            tabs = tabs(false),
            message = flash(saved.then_some("网关设置已保存"), error),
            csrf = esc(csrf),
            blacklist_selected = if settings.ua_mode == codex2api_storage::UaMode::Blacklist {
                "selected"
            } else {
                ""
            },
            whitelist_selected = if settings.ua_mode == codex2api_storage::UaMode::Whitelist {
                "selected"
            } else {
                ""
            },
            rules = esc(&settings.ua_rules.join("\n")),
        ),
    )
}

pub(crate) fn render(
    csrf: &str,
    old_username: &str,
    new_username: &str,
    error: Option<&str>,
) -> String {
    page(
        "账户安全设置",
        &nav("settings"),
        &format!(
            r#"{tabs}<section class="card admin-settings">
<h2>账户安全设置</h2>
{message}
<form class="stack" method="post" action="/admin/settings/security">
  <input type="hidden" name="csrf" value="{csrf}">
  <label>原用户名<input name="old_username" value="{old_username}" required autocomplete="username"></label>
  <label>原密码<input type="password" name="old_password" required autocomplete="current-password"></label>
  <label>新用户名<input name="new_username" value="{new_username}" required autocomplete="off"></label>
  <label>新密码<input type="password" name="new_password" autocomplete="new-password" aria-describedby="new-password-hint"></label>
  <p class="hint" id="new-password-hint">新密码留空则保留原密码。保存后需重新登录。</p>
  <div class="row-actions"><button type="submit">保存修改</button></div>
</form>
</section>"#,
            csrf = esc(csrf),
            tabs = tabs(true),
            old_username = esc(old_username),
            new_username = esc(new_username),
            message = flash(None, error),
        ),
    )
}

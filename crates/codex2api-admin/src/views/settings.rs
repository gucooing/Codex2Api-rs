use super::{esc, flash, nav, page};

pub(crate) fn render(
    csrf: &str,
    old_username: &str,
    new_username: &str,
    error: Option<&str>,
) -> String {
    page(
        "设置",
        &nav("settings"),
        &format!(
            r#"<section class="card admin-settings">
<h2>设置</h2>
{message}
<form class="stack" method="post" action="/admin/settings">
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
            old_username = esc(old_username),
            new_username = esc(new_username),
            message = flash(None, error),
        ),
    )
}

use super::{esc, flash, nav, page};
use crate::handlers::proxies::PageQuery;
use codex2api_storage::OutboundProxy;
use std::collections::HashMap;

fn check_result(
    ok: Option<bool>,
    latency: Option<i64>,
    error: Option<&str>,
    checked: Option<&str>,
    success: &str,
) -> String {
    let Some(ok) = ok else {
        return "<span class=\"muted\">未检测</span>".into();
    };
    let label = if ok { success } else { "失败" };
    let class = if ok { "active" } else { "disabled" };
    let latency = latency.map(|ms| format!("{ms} ms")).unwrap_or("—".into());
    let detail = error
        .map(|text| {
            format!(
                r#"<div class="muted proxy-check-detail">{}</div>"#,
                esc(text)
            )
        })
        .unwrap_or_default();
    let checked = checked
        .map(|time| {
            format!(
                r#"<time class="muted proxy-check-time" datetime="{}" data-proxy-time>{}</time>"#,
                esc(time),
                esc(time)
            )
        })
        .unwrap_or_default();
    format!(
        r#"<span class="badge {class}">{label}</span> <strong>{latency}</strong>{detail}{checked}"#
    )
}

pub(crate) fn row(proxy: &OutboundProxy, accounts: i64) -> String {
    let parsed = url::Url::parse(&proxy.url).ok();
    let protocol = parsed
        .as_ref()
        .map(|url| url.scheme().to_uppercase())
        .unwrap_or("—".into());
    let address = parsed
        .as_ref()
        .map(|url| {
            format!(
                "{}:{}",
                url.host_str().unwrap_or("—"),
                url.port_or_known_default().unwrap_or(1080)
            )
        })
        .unwrap_or("—".into());
    let auth = if parsed
        .as_ref()
        .is_some_and(|url| !url.username().is_empty() || url.password().is_some())
    {
        "已配置"
    } else {
        "无"
    };
    let mut places: Vec<&str> = Vec::new();
    for place in [
        proxy.country.as_deref(),
        proxy.region.as_deref(),
        proxy.city.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|s| !s.is_empty())
    {
        if !places.contains(&place) {
            places.push(place);
        }
    }
    let location = if places.is_empty() {
        "—".into()
    } else {
        esc(&places.join(" · "))
    };
    let ip = proxy
        .exit_ip
        .as_deref()
        .map(|ip| format!(r#"<div class="muted mono">{}</div>"#, esc(ip)))
        .unwrap_or_default();
    let connection = check_result(
        proxy.connection_ok,
        proxy.connection_latency_ms,
        proxy.connection_error.as_deref(),
        proxy.connection_checked_at.as_deref(),
        "成功",
    );
    let quality = check_result(
        proxy.quality_ok,
        proxy.quality_latency_ms,
        proxy.quality_error.as_deref(),
        proxy.quality_checked_at.as_deref(),
        "通过",
    );
    format!(
        r#"<tr data-proxy-row><td>{name}</td><td><span class="record-transport">{protocol}</span></td><td class="mono">{address}</td><td>{auth}</td><td>{location}{ip}</td><td>{accounts}</td><td>{connection}</td><td>{quality}</td><td><time datetime="{created}" data-proxy-time>{created}</time></td><td><div class="key-row-actions"><button class="secondary" type="button" data-proxy-check="/admin/proxies/{id}/test">测试连接</button><button class="secondary" type="button" data-proxy-check="/admin/proxies/{id}/quality">质量检测</button><button class="secondary" type="button" data-proxy-edit="/admin/proxies/{id}/edit">编辑</button><button class="danger" type="button" data-proxy-delete="/admin/proxies/{id}/delete">删除</button></div></td></tr>"#,
        name = esc(&proxy.name),
        address = esc(&address),
        id = esc(&proxy.id),
        created = esc(&proxy.created_at)
    )
}

pub(crate) fn render(
    proxies: &[OutboundProxy],
    counts: &HashMap<String, i64>,
    csrf: &str,
    query: &PageQuery,
) -> String {
    let mut rows = String::new();
    for proxy in proxies {
        rows.push_str(&row(proxy, counts.get(&proxy.id).copied().unwrap_or(0)));
    }
    if rows.is_empty() {
        rows.push_str(r#"<tr><td colspan="10" class="empty">暂无符合条件的代理。</td></tr>"#);
    }
    let mut options = String::from(r#"<option value="">全部协议</option>"#);
    let mut create_options = String::new();
    for protocol in ["http", "https", "socks5", "socks5h"] {
        let label = protocol.to_uppercase();
        options.push_str(&format!(
            r#"<option value="{protocol}" {}>{label}</option>"#,
            if query.protocol == protocol {
                "selected"
            } else {
                ""
            }
        ));
        create_options.push_str(&format!(r#"<option value="{protocol}">{label}</option>"#));
    }
    let body = format!(
        r#"{flash}
<div data-proxy-manager data-proxy-csrf="{csrf}">
<p class="flash err" data-proxy-error role="alert" hidden></p>
<section class="card"><div class="account-toolbar">
  <form class="record-filters proxy-filters" method="get" action="/admin/proxies">
    <label>名称 / 地址 / 地区<input name="keyword" value="{keyword}"></label>
    <label>协议<select name="protocol">{options}</select></label>
    <div class="record-filter-actions"><button type="submit">查询</button><a class="btn secondary" href="/admin/proxies">重置</a></div>
  </form>
  <div class="record-filter-actions account-add"><button type="button" data-add-proxy>添加代理</button></div>
</div></section>
<section class="card proxy-records"><div class="table-wrap"><table><thead><tr><th>名称</th><th>协议</th><th>地址</th><th>认证</th><th>地理位置</th><th>账户数</th><th>连接延迟</th><th>ChatGPT 质量 / 延迟</th><th>创建时间</th><th>操作</th></tr></thead><tbody>{rows}</tbody></table></div>
</section>
<dialog class="key-dialog proxy-dialog" id="create-proxy-dialog" aria-labelledby="create-proxy-title">
<form class="stack" method="post" action="/admin/proxies">
  <div class="key-heading"><h2 id="create-proxy-title">添加代理</h2><button type="button" class="ghost" data-proxy-cancel aria-label="关闭">×</button></div>
  <input type="hidden" name="csrf" value="{csrf}">
  <label>名称<input name="name" maxlength="128" required autofocus autocomplete="off" placeholder="请输入代理名称"></label>
  <label>协议<select name="protocol">{create_options}</select></label>
  <div class="fingerprint-fields"><label>主机<input name="host" required autocomplete="off" placeholder="请输入主机地址"></label><label>端口<input type="number" name="port" value="8080" min="1" max="65535" required></label></div>
  <label>用户名（可选）<input name="username" maxlength="255" autocomplete="off" placeholder="可选认证信息"></label>
  <label>密码（可选）<span class="proxy-password"><input type="password" name="password" maxlength="255" autocomplete="new-password" placeholder="可选认证信息"><button class="secondary" type="button" data-toggle-proxy-password aria-label="显示密码">显示</button></span></label>
  <label>绑定账户数量<output name="account_count" class="proxy-account-count">0</output></label>
  <div class="row-actions"><button class="secondary" type="button" data-proxy-cancel>取消</button><button type="submit">添加</button></div>
</form></dialog></div><script>{script}</script>"#,
        flash = flash(query.ok.as_deref(), query.err.as_deref()),
        csrf = esc(csrf),
        keyword = esc(&query.keyword),
        script = include_str!("../../static/proxies.js")
    );
    page("代理配置", &nav("proxies"), &body)
}

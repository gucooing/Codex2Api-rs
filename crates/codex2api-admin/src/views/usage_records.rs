use super::{esc, nav, page, usage::token_amount};
use crate::handlers::usage_records::Filters;
use codex2api_storage::{USAGE_PAGE_SIZE, UsageKeyOption, UsagePage, UsageRecord};

fn input(name: &str, label: &str, value: &str, kind: &str) -> String {
    format!(
        r#"<label>{label}<input type="{kind}" name="{name}" value="{}"></label>"#,
        esc(value)
    )
}
fn model(record: &UsageRecord) -> String {
    let requested = record.model.as_deref().filter(|name| !name.is_empty());
    let actual = record
        .actual_model
        .as_deref()
        .filter(|name| !name.is_empty());
    match (requested, actual) {
        (Some(requested), Some(actual)) if requested != actual => format!(
            r#"<strong class="record-model-changed">{}-&gt;{}</strong>"#,
            esc(requested),
            esc(actual)
        ),
        _ => format!("<strong>{}</strong>", esc(requested.unwrap_or("—"))),
    }
}
fn reasoning(record: &UsageRecord) -> String {
    let effort = esc(record.reasoning_effort.as_deref().unwrap_or("—"));
    let tier = match record
        .service_tier
        .as_deref()
        .filter(|tier| !tier.is_empty())
    {
        Some(tier) if tier.eq_ignore_ascii_case("default") => None,
        Some(tier) if tier.eq_ignore_ascii_case("priority") => Some("fast"),
        tier => tier,
    };
    match tier {
        Some(tier) => format!("{effort} · {}", esc(tier)),
        None => effort,
    }
}
fn tokens(label: &str, value: Option<i64>, class: &str) -> String {
    let text = value
        .filter(|n| *n >= 0)
        .map(|n| token_amount(n as u64))
        .unwrap_or("—".into());
    let exact = value
        .map(|n| format!("{n} Token"))
        .unwrap_or("官方未提供".into());
    format!(
        r#"<span class="record-token {class}" title="{}"><span>{label}</span><strong>{text}</strong></span>"#,
        esc(&exact)
    )
}
fn usage(record: &UsageRecord) -> String {
    if record.endpoint.ends_with("/alpha/search") {
        return "—".into();
    }
    if record.endpoint.contains("/images/") {
        return format!(
            "<span class=\"image-size\">{}</span>",
            esc(record.image_size.as_deref().unwrap_or("—"))
        );
    }
    let cache_rate = match (record.input_tokens, record.cached_tokens) {
        (Some(input), Some(cached)) if input > 0 && (0..=input).contains(&cached) => {
            format!("{:.1}%", cached as f64 / input as f64 * 100.0)
        }
        _ => "—".into(),
    };
    [
        tokens("输入", record.input_tokens, "token-input"),
        tokens("输出", record.output_tokens, "token-output"),
        tokens("缓存读取", record.cached_tokens, "token-cache"),
        tokens("缓存写入", record.cache_write_tokens, "token-cache"),
        tokens("思考", record.reasoning_tokens, "token-reasoning"),
        format!(r#"<span class="record-token token-cache" title="缓存读取 Token ÷ 输入 Token × 100%"><span>缓存率</span><strong>{cache_rate}</strong></span>"#),
    ]
    .join("")
}
fn timing(value: Option<i64>) -> String {
    let Some(ms) = value.filter(|n| *n >= 0) else {
        return "—".into();
    };
    if ms < 1000 {
        return format!("{ms}ms");
    }
    if ms < 60000 {
        return format!("{:.2}s", ms as f64 / 1000.0);
    }
    let seconds = ms / 1000;
    if seconds < 3600 {
        return format!("{}分{}秒", seconds / 60, seconds % 60);
    }
    if seconds < 86400 {
        return format!(
            "{}小时{}分{}秒",
            seconds / 3600,
            (seconds % 3600) / 60,
            seconds % 60
        );
    }
    format!(
        "{}天{}小时{}分{}秒",
        seconds / 86400,
        (seconds % 86400) / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}
fn status(record: &UsageRecord) -> String {
    let (label, class) = match record.status.as_str() {
        "in_progress" => ("进行中", "pending"),
        "completed" => ("完成", "active"),
        "failed" => ("失败", "disabled"),
        "incomplete" => ("未完成", "pending"),
        "interrupted" => ("已中断", "disabled"),
        _ => ("未知", "pending"),
    };
    let http = record
        .http_status
        .map(|s| format!(" · HTTP {s}"))
        .unwrap_or_default();
    format!(
        r#"<span class="badge {class}" title="{}{http}">{label}</span>"#,
        esc(&record.id)
    )
}
pub(crate) fn render(
    filters: &Filters,
    records: &UsagePage,
    keys: &[UsageKeyOption],
    accounts: &[String],
) -> String {
    let mut account_options = String::new();
    for account in accounts {
        account_options.push_str(&format!(r#"<option value="{}"></option>"#, esc(account)));
    }
    let mut status_options = String::new();
    for (value, label) in [
        ("", "全部状态"),
        ("in_progress", "进行中"),
        ("completed", "完成"),
        ("failed", "失败"),
        ("incomplete", "未完成"),
        ("interrupted", "已中断"),
    ] {
        status_options.push_str(&format!(
            r#"<option value="{value}" {}>{label}</option>"#,
            if value == filters.status {
                "selected"
            } else {
                ""
            },
        ));
    }
    let mut options = "<option value=\"\">全部 API Key</option>".to_string();
    for key in keys {
        options.push_str(&format!(
            r#"<option value="{}" {}>{} · {}</option>"#,
            esc(&key.id),
            if key.id == filters.api_key {
                "selected"
            } else {
                ""
            },
            esc(&key.name),
            esc(&key.account_name)
        ));
    }
    let mut body = format!(
        r#"<section class="card">
<form class="record-filters" method="get" action="/admin/usage" id="usage-filter-form">
<label>账户<input type="text" name="account" value="{}" list="usage-account-options" autocomplete="off"><datalist id="usage-account-options">{account_options}</datalist></label>
<label>API Key<select name="api_key">{options}</select></label>{}<label>状态<select name="status">{status_options}</select></label>{}{}
<input type="hidden" name="tz_offset" value="{}"><div class="record-filter-actions"><button type="submit">查询</button><a class="btn secondary" href="/admin/usage">重置</a></div>
</form></section>
<section class="card usage-records"><div class="table-wrap"><table><thead><tr><th>账户</th><th>API Key</th><th>模型 / 接口</th><th>推理强度</th><th>用量</th><th>耗时</th><th>请求时间</th><th>状态</th></tr></thead><tbody>"#,
        esc(&filters.account),
        input("model", "模型", &filters.model, "text"),
        input("from", "开始时间", &filters.from, "datetime-local"),
        input("until", "结束时间", &filters.until, "datetime-local"),
        filters.tz_offset
    );
    if records.records.is_empty() {
        body.push_str("<tr><td colspan=\"8\" class=\"empty\">暂无符合条件的用量记录</td></tr>");
    }
    for record in &records.records {
        let date = chrono::DateTime::from_timestamp_millis(record.requested_at_ms)
            .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or("—".into());
        let first = timing(record.first_byte_ms);
        let total = timing(record.total_ms);
        body.push_str(&format!(r#"<tr><td class="record-account" title="{}">{}</td><td class="record-key" title="{}">{}</td>
<td class="record-model">{}<div class="muted mono">{}</div><span class="record-transport">{}</span></td>
<td>{}</td><td class="record-tokens">{}</td><td class="record-timing"><div><span>首字节</span><strong>{first}</strong></div><div><span>总耗时</span><strong>{total}</strong></div></td>
<td class="record-time"><time data-request-time="{}">{date}</time></td><td>{}</td></tr>"#,
esc(&record.account_id),esc(&record.account_name),esc(&record.api_key_id),esc(&record.api_key_name),model(record),esc(&record.endpoint),if record.transport=="websocket"{"WebSocket"}else{"HTTP"},reasoning(record),usage(record),record.requested_at_ms,status(record)));
    }
    body.push_str(
        "</tbody></table></div><nav class=\"record-pagination\" aria-label=\"用量记录分页\">",
    );
    if records.page > 1 {
        body.push_str(&format!(
            r#"<a class="btn secondary" href="/admin/usage?{}">上一页</a>"#,
            esc(&filters.query_for_page(records.page - 1))
        ));
    }
    let pages = ((records.total.saturating_sub(1) / USAGE_PAGE_SIZE) + 1).max(1);
    body.push_str(&format!(
        "<span class=\"muted\">第 {} / {pages} 页 · 每页 {USAGE_PAGE_SIZE} 条</span>",
        records.page
    ));
    if i64::from(records.page) < pages {
        body.push_str(&format!(
            r#"<a class="btn secondary" href="/admin/usage?{}">下一页</a>"#,
            esc(&filters.query_for_page(records.page + 1))
        ));
    }
    body.push_str("</nav></section><script>");
    body.push_str(include_str!("../../static/usage-records.js"));
    body.push_str("</script>");
    page("用量管理", &nav("usage-records"), &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn format_usage_without_inventing_missing_tokens_or_leaking_html() {
        assert_eq!(timing(Some(1234)), "1.23s");
        assert_eq!(timing(Some(90123)), "1分30秒");
        assert_eq!(timing(None), "—");
        let mut record = UsageRecord {
            endpoint: "/v1/alpha/search".into(),
            ..Default::default()
        };
        assert_eq!(usage(&record), "—");
        record.endpoint = "/v1/images/edits".into();
        record.image_size = Some("<size>".into());
        assert!(usage(&record).contains("&lt;size&gt;"));
        record.endpoint = "/v1/responses".into();
        record.reasoning_effort = Some("xhigh".into());
        record.service_tier = Some("priority".into());
        assert_eq!(reasoning(&record), "xhigh · fast");
        record.service_tier = Some("flex".into());
        assert_eq!(reasoning(&record), "xhigh · flex");
        record.service_tier = Some("ultrafast".into());
        assert_eq!(reasoning(&record), "xhigh · ultrafast");
        record.service_tier = Some("<future>".into());
        assert_eq!(reasoning(&record), "xhigh · &lt;future&gt;");
        record.service_tier = Some("default".into());
        assert_eq!(reasoning(&record), "xhigh");
        record.input_tokens = Some(0);
        assert!(usage(&record).contains("<strong>0</strong>"));
        assert!(usage(&record).contains("<strong>—</strong>"));
        assert!(usage(&record).contains("<span>缓存率</span><strong>—</strong>"));
        record.input_tokens = Some(33630);
        record.cached_tokens = Some(29440);
        assert!(usage(&record).contains("<span>缓存率</span><strong>87.5%</strong>"));
        record.cached_tokens = Some(0);
        assert!(usage(&record).contains("<span>缓存率</span><strong>0.0%</strong>"));
        let page = render(
            &Filters::default(),
            &UsagePage {
                records: vec![UsageRecord {
                    account_name: "<script>".into(),
                    ..record
                }],
                total: 1,
                page: 1,
            },
            &[],
            &[],
        );
        assert!(page.contains("&lt;script&gt;"));
        assert!(page.contains("账户管理"));
        assert!(page.contains("用量管理"));
    }
}

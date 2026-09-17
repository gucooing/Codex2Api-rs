use crate::models::Section;
use crate::views::esc;
use codex2api_storage::Account;
use codex2api_upstream::BackendEndpoint as E;
use serde_json::Value;

fn scalar(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "—".into(),
        Some(Value::String(s)) => esc(s),
        Some(v) => esc(&v.to_string()),
    }
}

fn label(key: &str) -> &str {
    match key {
        "account_id" => "账户 ID",
        "user_id" => "用户 ID",
        "plan_type" => "套餐",
        "name" => "名称",
        "title" => "标题",
        "accounts" => "账户",
        "account" => "账户",
        "default_account_id" => "默认账户",
        "account_ordering" => "账户顺序",
        "structure" => "账户类型",
        "stats" => "使用统计",
        "lifetime_tokens" => "累计 Token",
        "peak_daily_tokens" => "单日最高 Token",
        "longest_running_turn_sec" => "最长回合时长（秒）",
        "current_streak_days" => "当前连续使用天数",
        "longest_streak_days" => "最长连续使用天数",
        "daily_usage_buckets" => "每日用量",
        "start_date" => "日期",
        "tokens" => "Token",
        "credits" => "额度",
        "rate_limit" => "使用限额",
        "primary_window" => "主要窗口",
        "secondary_window" => "次要窗口",
        "used_percent" => "已用比例（%）",
        "reset_at" => "重置时间",
        "reset_after_seconds" => "距重置秒数",
        "limit_window_seconds" => "窗口长度（秒）",
        "allowed" => "允许使用",
        "limit_reached" => "已达到限额",
        "balance" => "余额",
        "unlimited" => "不限量",
        "has_credits" => "有可用额度",
        "additional_rate_limits" => "其他限额",
        "rate_limit_reset_credits" => "可重置额度",
        "available_count" => "可用次数",
        "code_review_rate_limit" => "代码审查限额",
        "status" => "状态",
        "created_at" => "创建时间",
        "updated_at" => "更新时间",
        "archived" => "已归档",
        "output_items" => "输出",
        "input_items" => "输入",
        "content" => "内容",
        "text" => "文本",
        "diff" => "代码差异",
        "description" => "说明",
        "expires_at" => "到期时间",
        "granted_at" => "发放时间",
        "reset_type" => "重置类型",
        _ => key,
    }
}

/// Render upstream data as escaped fields/tables, including future optional fields.
fn value_html(value: &Value) -> String {
    match value {
        Value::Object(map) if map.is_empty() => "<p class=\"empty\">暂无数据</p>".into(),
        Value::Object(map) => {
            let mut html = "<dl class=\"dl official-fields\">".to_string();
            for (key, value) in map {
                let rendered = if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "access_token"
                        | "refresh_token"
                        | "id_token"
                        | "authorization"
                        | "openai_api_key"
                        | "client_secret"
                ) {
                    "已隐藏".into()
                } else {
                    value_html(value)
                };
                html.push_str(&format!("<dt>{}</dt><dd>{rendered}</dd>", esc(label(key))));
            }
            html.push_str("</dl>");
            html
        }
        Value::Array(rows) if rows.is_empty() => "<span class=\"muted\">暂无数据</span>".into(),
        Value::Array(rows) => rows
            .iter()
            .map(|v| format!("<div class=\"official-item\">{}</div>", value_html(v)))
            .collect(),
        Value::String(text) if text.contains('\n') => {
            format!("<pre class=\"urlbox\">{}</pre>", esc(text))
        }
        Value::Bool(flag) => {
            if *flag {
                "是".into()
            } else {
                "否".into()
            }
        }
        _ => scalar(Some(value)),
    }
}

fn hidden(name: &str, value: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
        esc(name),
        esc(value)
    )
}
fn credits(value: &Value, id: &str, csrf: &str) -> String {
    let mut out = format!("<p>可用次数：{}</p>", scalar(value.get("available_count")));
    if let Some(items) = value.get("credits").and_then(Value::as_array) {
        out.push_str("<div class=\"table-wrap\"><table><thead><tr><th>名称</th><th>类型</th><th>状态</th><th>到期</th><th>操作</th></tr></thead><tbody>");
        for item in items {
            let button = if item.get("status").and_then(Value::as_str) == Some("available") {
                consume_form(
                    id,
                    csrf,
                    item.get("id").and_then(Value::as_str).unwrap_or(""),
                )
            } else {
                "—".into()
            };
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{button}</td></tr>",
                scalar(
                    item.get("title")
                        .filter(|v| !v.is_null())
                        .or_else(|| item.get("id"))
                ),
                scalar(item.get("reset_type")),
                scalar(item.get("status")),
                scalar(item.get("expires_at"))
            ));
        }
        out.push_str("</tbody></table></div>");
    }
    if value
        .get("available_count")
        .and_then(Value::as_i64)
        .is_some_and(|n| n > 0)
    {
        out.push_str("<p class=\"hint\">使用一次可用额度重置，具体可重置的窗口由官方决定。</p>");
        out.push_str(&consume_form(id, csrf, ""));
    }
    out
}
fn consume_form(id: &str, csrf: &str, credit: &str) -> String {
    format!(
        "<form class=\"inline-form\" method=\"post\" action=\"/admin/accounts/{}/official\">{}{}{}{}<button type=\"submit\">使用重置额度</button></form>",
        esc(id),
        hidden("csrf", csrf),
        hidden("action", "consume"),
        hidden("credit_id", credit),
        hidden("redeem_request_id", &uuid::Uuid::new_v4().to_string())
    )
}

pub(crate) fn sections(account: &Account, sections: &[Section], csrf: &str) -> String {
    let id = &account.id;
    let mut body = String::new();
    for section in sections {
        let content = match &section.result {
            Err(error) => format!("<div class=\"flash err\">{}</div>", esc(error)),
            Ok(value) => match section.endpoint {
                E::Profile => super::usage::profile_chart(value),
                E::Credits => credits(value, id, csrf),
                _ => value_html(value),
            },
        };
        body.push_str(&format!(
            "<section class=\"card\"><h2>{}</h2>{content}</section>",
            esc(section.title)
        ));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn upstream_data_escapes_text_and_hides_tokens() {
        let html = value_html(&json!({"access_token":"secret", "name":"<script>"}));
        assert!(!html.contains("secret"));
        assert!(html.contains("&lt;script&gt;"));
    }
    #[test]
    fn credit_actions_require_explicit_posts() {
        let html = credits(&json!({"credits":[],"available_count":1}), "a", "csrf");
        assert!(html.contains("method=\"post\""));
        assert!(html.contains("redeem_request_id"));
        assert!(html.contains("name=\"csrf\""));
        assert!(
            !credits(&json!({"credits":[],"available_count":0}), "a", "csrf").contains("<form")
        );
    }
}

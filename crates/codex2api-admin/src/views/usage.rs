use super::esc;
use crate::models::{DailyUsage, QuotaWindow, UsageProfile, quota_window};
use chrono::{DateTime, Utc};
use serde_json::Value;

const SCRIPT: &str = include_str!("../../static/usage.js");

pub(crate) fn quota_inline(
    result: Option<&Result<Value, String>>,
    observed_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> String {
    let mut windows = Vec::new();
    for (label, duration) in [("周", 7 * 24 * 60 * 60), ("5h", 5 * 60 * 60)] {
        let window = result
            .and_then(|result| result.as_ref().ok())
            .and_then(|value| quota_window(value, duration, observed_at));
        let reset = match window.as_ref().and_then(|window| window.reset_at) {
            Some(reset) => format!(
                r#"<span class="account-quota-reset" data-reset-countdown="{}" data-reset-elapsed="请刷新额度">{}</span>"#,
                reset.timestamp(),
                if reset <= now {
                    "请刷新额度".into()
                } else {
                    reset_remaining(reset.timestamp() - now.timestamp())
                },
            ),
            None => format!(
                r#"<span class="account-quota-reset">{}</span>"#,
                if result.is_none() {
                    "—"
                } else {
                    "重置时间未提供"
                }
            ),
        };
        let (title, percentage, bar) = match window.and_then(|window| window.remaining_percent) {
            Some(value) => {
                let percent = format!("{value:.1}")
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string();
                (
                    format!("{label}剩余额度 {percent}%"),
                    format!("{percent}%"),
                    format!(
                        r#"<span class="quota-track" role="progressbar" aria-label="{label}剩余额度" aria-valuemin="0" aria-valuemax="100" aria-valuenow="{percent}"><span style="width:{percent}%"></span></span>"#
                    ),
                )
            }
            None => (
                match result {
                    None => "正在加载额度".to_string(),
                    Some(Err(error)) => error.clone(),
                    Some(Ok(_)) => "官方暂未提供此额度".to_string(),
                },
                "—".to_string(),
                r#"<span class="quota-track unavailable" aria-hidden="true"></span>"#.to_string(),
            ),
        };
        windows.push(format!(
            r#"<span class="account-quota-window" title="{}"><span class="account-quota-line"><span>{label}：</span>{bar}<span class="quota-percent">{percentage}</span></span>{reset}</span>"#,
            esc(&title)
        ));
    }
    format!(
        r#"<span class="account-quota">{}</span>"#,
        windows.join("<span class=\"muted\">；</span>")
    )
}

pub(crate) fn quota_panel(
    account_id: &str,
    result: &Result<Value, String>,
    observed_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> String {
    let value = result.as_ref().ok();
    let week = value.and_then(|value| quota_window(value, 7 * 24 * 60 * 60, observed_at));
    let short = value.and_then(|value| quota_window(value, 5 * 60 * 60, observed_at));
    let error = result
        .as_ref()
        .err()
        .map(|e| format!("<p class=\"flash err\">{}</p>", esc(e)))
        .unwrap_or_default();
    format!(
        r#"<section class="card quota-panel" aria-labelledby="quota-heading">
  <div class="usage-heading"><h2 id="quota-heading">套餐限额</h2><a href="/admin/accounts/{id}?tab=usage">查看用量明细</a></div>
  {error}<div class="quota-grid">{week}{short}</div>
  <div class="quota-footer"><span class="muted">进度条表示剩余额度</span><a href="/admin/accounts/{id}?tab=credits">使用限额重置</a></div>
</section><script>{SCRIPT}</script>"#,
        id = esc(account_id),
        week = quota_card("每周限额", week.as_ref(), now),
        short = quota_card("5 小时限额", short.as_ref(), now)
    )
}

fn quota_card(label: &str, window: Option<&QuotaWindow>, now: DateTime<Utc>) -> String {
    let (percentage, bar) = match window.and_then(|w| w.remaining_percent) {
        Some(value) => {
            let percent = format!("{value:.1}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string();
            (
                format!("剩余 {percent}%"),
                format!(
                    r#"<div class="quota-track" role="progressbar" aria-label="{label}剩余额度" aria-valuemin="0" aria-valuemax="100" aria-valuenow="{percent}"><span style="width:{percent}%"></span></div>"#
                ),
            )
        }
        None => (
            "剩余 —".into(),
            "<div class=\"quota-track unavailable\" aria-hidden=\"true\"></div>".into(),
        ),
    };
    let reset = match window.and_then(|w| w.reset_at) {
        Some(reset) => format!(
            r#"<span data-reset-countdown="{timestamp}">{relative}</span><time datetime="{iso}" data-local-time="{timestamp}">{absolute} UTC</time>"#,
            timestamp = reset.timestamp(),
            relative = reset_remaining(reset.timestamp() - now.timestamp()),
            iso = reset.to_rfc3339(),
            absolute = reset.format("%m-%d %H:%M")
        ),
        None => {
            if window.is_some() {
                "<span>重置时间未提供</span>".into()
            } else {
                "<span>官方暂未提供此额度</span>".into()
            }
        }
    };
    format!(
        r#"<div class="quota-window"><h3>{label}</h3><div class="quota-line"><div class="quota-reset">{reset}</div><span class="quota-percent">{percentage}</span></div>{bar}</div>"#
    )
}

fn reset_remaining(seconds: i64) -> String {
    if seconds <= 0 {
        return "已到重置时间，请刷新额度".into();
    }
    let minutes = seconds / 60 + i64::from(seconds % 60 != 0);
    if minutes >= 1440 {
        format!("{} 天 {} 小时后重置", minutes / 1440, (minutes % 1440) / 60)
    } else if minutes >= 60 {
        format!("{} 小时 {} 分钟后重置", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} 分钟后重置")
    }
}

fn number(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub(crate) fn token_amount(value: u64) -> String {
    const UNITS: &[(u64, &str)] = &[
        (1, ""),
        (1_000, "K"),
        (1_000_000, "M"),
        (1_000_000_000, "B"),
        (1_000_000_000_000, "T"),
    ];
    let mut index = UNITS
        .iter()
        .rposition(|(scale, _)| value >= *scale)
        .unwrap_or(0);
    if index == 0 {
        return value.to_string();
    }
    loop {
        let (scale, suffix) = UNITS[index];
        let hundredths = (u128::from(value) * 100 + u128::from(scale) / 2) / u128::from(scale);
        if hundredths >= 100_000 && index + 1 < UNITS.len() {
            index += 1;
            continue;
        }
        let fraction = format!("{:02}", hundredths % 100);
        let fraction = fraction.trim_end_matches('0');
        return if fraction.is_empty() {
            format!("{}{suffix}", hundredths / 100)
        } else {
            format!("{}.{fraction}{suffix}", hundredths / 100)
        };
    }
}

fn duration(mut seconds: u64) -> String {
    let mut parts = String::new();
    for (size, unit) in [(86400, "天"), (3600, "小时"), (60, "分"), (1, "秒")] {
        let amount = seconds / size;
        seconds %= size;
        if amount != 0 {
            parts.push_str(&format!("{amount}{unit}"));
        }
    }
    if parts.is_empty() {
        "0秒".into()
    } else {
        parts
    }
}

enum MetricUnit {
    Tokens,
    Days,
    Seconds,
}

fn metric(label: &str, value: Option<u64>, unit: MetricUnit) -> String {
    let (text, exact) = value
        .map(|n| match unit {
            MetricUnit::Tokens => (token_amount(n), format!("{} Token", number(n))),
            MetricUnit::Days => (format!("{} 天", number(n)), format!("{} 天", number(n))),
            MetricUnit::Seconds => (duration(n), format!("{} 秒", number(n))),
        })
        .unwrap_or(("—".into(), "官方未提供".into()));
    format!(
        "<div class=\"usage-metric\"><dt>{label}</dt><dd title=\"{}\">{text}</dd></div>",
        esc(&exact)
    )
}

pub(crate) fn profile_chart(value: &Value) -> String {
    let Some(stats) = value.get("stats").filter(|stats| stats.is_object()) else {
        return "<p class=\"empty\">官方暂未提供用量明细</p>".into();
    };
    let Ok(mut profile) = serde_json::from_value::<UsageProfile>(stats.clone()) else {
        return "<p class=\"empty\">官方用量数据格式暂无法显示</p>".into();
    };
    let mut out = "<dl class=\"usage-metrics\">".to_string();
    out.push_str(&metric(
        "累计 Token",
        profile.lifetime_tokens,
        MetricUnit::Tokens,
    ));
    out.push_str(&metric(
        "单日最高 Token",
        profile.peak_daily_tokens,
        MetricUnit::Tokens,
    ));
    out.push_str(&metric(
        "当前连续使用",
        profile.current_streak_days,
        MetricUnit::Days,
    ));
    out.push_str(&metric(
        "最长连续使用",
        profile.longest_streak_days,
        MetricUnit::Days,
    ));
    out.push_str(&metric(
        "最长回合时长",
        profile.longest_running_turn_sec,
        MetricUnit::Seconds,
    ));
    out.push_str("</dl>");
    match profile
        .daily_usage_buckets
        .as_mut()
        .filter(|days| !days.is_empty())
    {
        Some(days) => {
            days.sort_by_key(|day| day.start_date);
            out.push_str(&daily_chart(days));
        }
        None => out.push_str("<div class=\"usage-chart-empty\">暂无每日用量数据</div>"),
    }
    out.push_str(&format!("<script>{SCRIPT}</script>"));
    out
}

fn axis_label(value: f64) -> String {
    token_amount(value.round() as u64)
}

fn daily_chart(days: &[DailyUsage]) -> String {
    use std::fmt::Write;
    let width = (days.len() * 24 + 84).max(680);
    let plot_width = (width - 84) as f64;
    let step = plot_width / days.len() as f64;
    let maximum = days
        .iter()
        .filter_map(|day| day.tokens)
        .max()
        .unwrap_or(0)
        .max(4);
    let ceiling = (maximum as f64 / 4.0).ceil() * 4.0;
    let range = format!(
        "{} — {}",
        days.first().unwrap().start_date,
        days.last().unwrap().start_date
    );
    let mut out = format!(
        r#"<figure class="usage-chart" aria-labelledby="daily-usage-heading">
<div class="usage-heading"><h3 id="daily-usage-heading">每日 Token 用量</h3><span class="muted">{range}</span></div>
<div class="usage-chart-scroll"><svg class="usage-plot" viewBox="0 0 {width} 264" style="min-width:{width}px" role="img" aria-labelledby="daily-chart-title daily-chart-desc">
<title id="daily-chart-title">每日 Token 用量柱状图</title><desc id="daily-chart-desc">横轴为日期，纵轴为 Token 数量。聚焦各日期可查看具体用量。</desc>"#
    );
    for tick in 0..=4 {
        let y = 216 - tick * 46;
        write!(out,r#"<line class="usage-grid-line" x1="64" y1="{y}" x2="{}" y2="{y}"/><text class="usage-axis" x="54" y="{}" text-anchor="end">{}</text>"#,width-20,y+4,axis_label(ceiling*f64::from(tick)/4.0)).unwrap();
    }
    let label_step = (64.0 / step).ceil().max(1.0) as usize;
    for (index, day) in days.iter().enumerate() {
        let x = 64.0 + index as f64 * step;
        let bar_width = (step * 0.6).min(40.0);
        let height = day
            .tokens
            .map(|tokens| tokens as f64 / ceiling * 184.0)
            .unwrap_or(0.0);
        let text = format!(
            "{} · {}",
            day.start_date,
            day.tokens
                .map(|tokens| if tokens < 1_000 {
                    format!("{} Token", token_amount(tokens))
                } else {
                    format!("{} Token（{} Token）", token_amount(tokens), number(tokens))
                })
                .unwrap_or("未提供".into())
        );
        write!(out,r#"<g class="usage-day" tabindex="0" aria-label="{}" data-usage-tooltip="{}"><title>{}</title><rect class="usage-hit" x="{x:.2}" y="28" width="{step:.2}" height="190"/><rect class="usage-bar" x="{:.2}" y="{:.2}" width="{bar_width:.2}" height="{height:.2}" rx="2"/>"#,esc(&text),esc(&text),esc(&text),x+(step-bar_width)/2.0,216.0-height).unwrap();
        if day.tokens.is_none() {
            write!(
                out,
                r#"<text class="usage-axis" x="{:.2}" y="211" text-anchor="middle">—</text>"#,
                x + step / 2.0
            )
            .unwrap();
        }
        out.push_str("</g>");
        if index.is_multiple_of(label_step)
            || (index == days.len() - 1 && index % label_step > label_step / 2)
        {
            write!(
                out,
                r#"<text class="usage-axis" x="{:.2}" y="241" text-anchor="middle">{}</text>"#,
                x + step / 2.0,
                day.start_date.format("%m/%d")
            )
            .unwrap();
        }
    }
    out.push_str("</svg></div><div class=\"usage-tooltip\" role=\"tooltip\" hidden></div><figcaption class=\"muted\">悬停或聚焦柱形可查看每日用量，横向滚动查看其他日期。</figcaption></figure>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn quotas_use_remaining_percent_and_match_actual_window_lengths() {
        let now = DateTime::from_timestamp(2_000_000_000, 0).unwrap();
        let value = json!({"rate_limit":{"primary_window":{"used_percent":58,"limit_window_seconds":604800,"reset_at":2_000_100_000},
            "secondary_window":{"used_percent":20,"limit_window_seconds":18000,"reset_after_seconds":3600}}});
        let html = quota_panel(
            "account",
            &Ok(value),
            now,
            now + chrono::TimeDelta::minutes(5),
        );
        assert!(html.contains("剩余 42%"));
        assert!(html.contains("width:42%"));
        assert!(html.contains("剩余 80%"));
        assert!(html.contains("data-reset-countdown=\"2000003600\""));
        let absent = quota_panel("account", &Ok(json!({"rate_limit":null})), now, now);
        assert!(absent.contains("官方暂未提供此额度"));
        assert!(!absent.contains("aria-valuenow"));
        let error = quota_panel("account", &Err("<unavailable>".into()), now, now);
        assert!(error.contains("&lt;unavailable&gt;"));
        assert!(!error.contains("剩余 100%"));
    }
    #[test]
    fn inline_reset_times_use_the_snapshot_age_and_keep_missing_times_unknown() {
        let observed = DateTime::from_timestamp(2_000_000_000, 0).unwrap();
        let now = observed + chrono::TimeDelta::minutes(5);
        let quota = Ok(json!({"rate_limit": {
            "primary_window": {"used_percent": 58, "limit_window_seconds": 604800, "reset_at": 2_000_172_800_i64},
            "secondary_window": {"used_percent": 20, "limit_window_seconds": 18000, "reset_after_seconds": 3600}
        }}));
        let html = quota_inline(Some(&quota), observed, now);
        assert!(html.contains("1 天 23 小时后重置"));
        assert!(html.contains("55 分钟后重置"));
        assert!(html.contains("data-reset-countdown=\"2000003600\""));
        assert!(html.contains("42%"));
        assert!(html.contains("80%"));
        let expired = quota_inline(
            Some(&quota),
            observed,
            observed + chrono::TimeDelta::hours(1),
        );
        assert!(expired.contains("请刷新额度</span>"));
        let missing = quota_inline(Some(&Ok(json!({"rate_limit":null}))), observed, now);
        assert_eq!(missing.matches("重置时间未提供").count(), 2);
        assert!(!missing.contains("data-reset-countdown"));
    }

    #[test]
    fn quota_clamps_overuse_without_inventing_a_reset_time() {
        let value = json!({"rate_limit":{"primary_window":{"used_percent":120,"limit_window_seconds":18000}}});
        let now = Utc::now();
        let html = quota_panel("a", &Ok(value), now, now);
        assert!(html.contains("剩余 0%"));
        assert!(html.contains("重置时间未提供"));
    }
    #[test]
    fn charts_preserve_dates_zero_counts_and_missing_data() {
        let html = profile_chart(
            &json!({"stats":{"lifetime_tokens":12345,"daily_usage_buckets":[
            {"start_date":"2026-09-02","tokens":null},{"start_date":"2026-09-01","tokens":0},{"start_date":"2026-09-03","tokens":12345}]}}),
        );
        assert!(html.contains("12,345"));
        assert!(html.contains("2026-09-01 · 0 Token"));
        assert!(html.contains("2026-09-02 · 未提供"));
        assert!(html.contains("2026-09-03 · 12.35K Token（12,345 Token）"));
        assert!(html.find("2026-09-01 ·").unwrap() < html.find("2026-09-02 ·").unwrap());
        assert!(html.contains("<svg"));
        let svg = html
            .split("<svg")
            .nth(1)
            .unwrap()
            .split("</svg>")
            .next()
            .unwrap();
        assert!(!svg.contains("NaN"));
        let missing = profile_chart(&json!({"stats":{}}));
        assert!(missing.contains("暂无每日用量数据"));
        assert!(!missing.contains("<svg"));
        assert!(profile_chart(&json!({})).contains("官方暂未提供用量明细"));
    }

    #[test]
    fn token_and_duration_units_match_the_displayed_values() {
        for (value, expected) in [
            (0, "0"),
            (999, "999"),
            (1000, "1K"),
            (12345, "12.35K"),
            (999_999, "1M"),
            (18_102_455_956, "18.1B"),
            (2_526_012_716, "2.53B"),
        ] {
            assert_eq!(token_amount(value), expected);
        }
        assert_eq!(duration(55_113), "15小时18分33秒");
        assert_eq!(duration(90_061), "1天1小时1分1秒");
        assert_eq!(duration(0), "0秒");
        assert_eq!(duration(3600), "1小时");
        let html = profile_chart(
            &json!({"stats":{"lifetime_tokens":18_102_455_956_u64,"longest_running_turn_sec":55_113}}),
        );
        assert!(html.contains("title=\"18,102,455,956 Token\">18.1B"));
        assert!(html.contains("title=\"55,113 秒\">15小时18分33秒"));
    }
}

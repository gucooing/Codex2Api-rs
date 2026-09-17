//! Data exchanged between official-account handlers and views.
use codex2api_upstream::BackendEndpoint as E;
use serde::Deserialize;
use serde_json::Value;

#[derive(Default, Deserialize)]
pub(crate) struct PageQuery {
    #[serde(default = "default_tab")]
    pub tab: String,
    pub ok: Option<String>,
    pub err: Option<String>,
}
fn default_tab() -> String {
    "usage".into()
}

pub(crate) struct Section {
    pub title: &'static str,
    pub endpoint: E,
    pub result: Result<Value, String>,
}

/// Official windows are identified by their duration, not primary/secondary position.
pub(crate) struct QuotaWindow {
    pub remaining_percent: Option<f64>,
    pub reset_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub(crate) fn quota_window(
    value: &Value,
    duration: i64,
    observed: chrono::DateTime<chrono::Utc>,
) -> Option<QuotaWindow> {
    let limits = value.get("rate_limit")?;
    let window = ["primary_window", "secondary_window"]
        .into_iter()
        .filter_map(|key| limits.get(key))
        .find(|window| {
            window.get("limit_window_seconds").and_then(Value::as_i64) == Some(duration)
        })?;
    let used = window
        .get("used_percent")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite());
    let reset_at = window
        .get("reset_at")
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .and_then(|n| chrono::DateTime::from_timestamp(n, 0))
        .or_else(|| {
            window
                .get("reset_after_seconds")
                .and_then(Value::as_i64)
                .filter(|n| *n >= 0)
                .and_then(|n| chrono::TimeDelta::try_seconds(n))
                .and_then(|delay| observed.checked_add_signed(delay))
        });
    Some(QuotaWindow {
        remaining_percent: used.map(|n| (100.0 - n).clamp(0.0, 100.0)),
        reset_at,
    })
}

#[derive(Deserialize)]
pub(crate) struct UsageProfile {
    pub lifetime_tokens: Option<u64>,
    pub peak_daily_tokens: Option<u64>,
    pub longest_running_turn_sec: Option<u64>,
    pub current_streak_days: Option<u64>,
    pub longest_streak_days: Option<u64>,
    pub daily_usage_buckets: Option<Vec<DailyUsage>>,
}

#[derive(Deserialize)]
pub(crate) struct DailyUsage {
    pub start_date: chrono::NaiveDate,
    pub tokens: Option<u64>,
}

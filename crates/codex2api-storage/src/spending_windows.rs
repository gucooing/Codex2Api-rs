use crate::{Result, Storage, StorageError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// 单链额度的一层；按顶层到最内层排列，金额留空表示这一层不限额。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpendingWindow {
    pub duration_seconds: i64,
    pub cost_limit_usd: Option<String>,
}

pub fn spending_windows(value: &Value) -> Result<Vec<SpendingWindow>> {
    let windows: Vec<SpendingWindow> = serde_json::from_value(value.clone())
        .map_err(|_| StorageError::Constraint("额度规则必须包含时长和费用上限".into()))?;
    if windows.len() > 2 {
        return Err(StorageError::Constraint(
            "套餐最多配置外层和内层两个额度窗口".into(),
        ));
    }
    let mut parent = None;
    for window in &windows {
        let valid_duration = match parent {
            None => matches!(window.duration_seconds, 604800 | 2592000),
            Some(outer) => window.duration_seconds == 18000 && window.duration_seconds <= outer,
        };
        if !valid_duration {
            return Err(StorageError::Constraint(
                "外层额度只能是 7 天或 30 天，内层额度只能是 5 小时".into(),
            ));
        }
        if window
            .cost_limit_usd
            .as_ref()
            .is_some_and(|value| crate::decimal_units(value, 9).is_none())
        {
            return Err(StorageError::Constraint(
                "额度须为非负美元金额，最多九位小数".into(),
            ));
        }
        parent = Some(window.duration_seconds);
    }
    Ok(windows)
}

/// Read the new nested form, with a legacy fallback for databases created before
/// the nested plan fields were introduced.
pub fn plan_spending_windows(config: &Value, free: bool) -> Result<Vec<SpendingWindow>> {
    let nested_key = if free {
        "free_spending_windows"
    } else {
        "spending_windows"
    };
    let primary_key = if free {
        "free_primary_cost_limit_usd"
    } else {
        "primary_cost_limit_usd"
    };
    let weekly_key = if free {
        "free_weekly_cost_limit_usd"
    } else {
        "weekly_cost_limit_usd"
    };
    // Legacy plans keep the old fields so historical tests and migrated data keep
    // their exact values. New plans omit those fields and use the nested form.
    if config.get(nested_key).is_some()
        && config.get(primary_key).is_none()
        && config.get(weekly_key).is_none()
    {
        return spending_windows(&config[nested_key]);
    }
    let weekly = config.get(weekly_key).cloned().unwrap_or(Value::Null);
    let primary = config.get(primary_key).cloned().unwrap_or(Value::Null);
    if weekly.is_null() && primary.is_null() {
        return Ok(Vec::new());
    }
    let mut result = vec![SpendingWindow {
        duration_seconds: 604800,
        cost_limit_usd: (!weekly.is_null()).then(|| {
            weekly
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| weekly.to_string())
        }),
    }];
    if !primary.is_null() {
        result.push(SpendingWindow {
            duration_seconds: 18000,
            cost_limit_usd: Some(
                primary
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| primary.to_string()),
            ),
        });
    }
    Ok(result)
}

impl Storage {
    /// 只读账本推导内层起点，不因打开额度页而启动周期或取得写锁。
    pub(crate) async fn nested_spending_windows(
        &self,
        owner: &str,
        rules: &[SpendingWindow],
        anchor: i64,
        now: i64,
    ) -> Result<Vec<Value>> {
        let Some(root) = rules.first() else {
            return Ok(Vec::new());
        };
        let root_start = anchor
            .checked_add(
                now.saturating_sub(anchor).max(0) / root.duration_seconds * root.duration_seconds,
            )
            .ok_or_else(|| StorageError::Constraint("额度周期超出时间范围".into()))?;
        let from_ms = root_start
            .checked_mul(1000)
            .ok_or_else(|| StorageError::Constraint("额度周期超出时间范围".into()))?;
        let until_ms = now
            .checked_add(1)
            .and_then(|v| v.checked_mul(1000))
            .ok_or_else(|| StorageError::Constraint("额度周期超出时间范围".into()))?;
        let usage: Vec<(i64, Option<i64>)> = sqlx::query_as(
            "SELECT requested_at_ms, cost_nano_usd FROM usage_records
             WHERE subject_id=? AND requested_at_ms>=? AND requested_at_ms<?
             ORDER BY requested_at_ms,id",
        )
        .bind(owner)
        .bind(from_ms)
        .bind(until_ms)
        .fetch_all(self.pool())
        .await?;
        let mut parent_start = Some(root_start);
        let mut parent_end = None;
        let mut parent_remaining: Option<i64> = None;
        let mut result = Vec::with_capacity(rules.len());
        for (depth, rule) in rules.iter().enumerate() {
            let start = if depth == 0 {
                Some(root_start)
            } else if let Some(parent_start) = parent_start {
                let mut start = None;
                for (at_ms, _) in &usage {
                    let at = at_ms.div_euclid(1000);
                    if at < parent_start {
                        continue;
                    }
                    if start.is_none_or(|previous: i64| {
                        at.saturating_sub(previous) >= rule.duration_seconds
                    }) {
                        start = Some(at);
                    }
                }
                start.filter(|start| now.saturating_sub(*start) < rule.duration_seconds)
            } else {
                None
            };
            let end = start
                .map(|start| {
                    start
                        .checked_add(rule.duration_seconds)
                        .map(|end| parent_end.map_or(end, |parent: i64| parent.min(end)))
                        .ok_or_else(|| StorageError::Constraint("额度周期超出时间范围".into()))
                })
                .transpose()?;
            let used = usage
                .iter()
                .filter(|(at, _)| start.is_some_and(|start| at.div_euclid(1000) >= start))
                .try_fold(0_i64, |sum, (_, cost)| sum.checked_add(cost.unwrap_or(0)))
                .ok_or_else(|| StorageError::Constraint("额度用量超出金额范围".into()))?;
            let limit = rule
                .cost_limit_usd
                .as_deref()
                .and_then(|v| crate::decimal_units(v, 9));
            let remaining = limit.map(|limit| limit.saturating_sub(used).max(0));
            let effective_remaining = match (parent_remaining, remaining) {
                (Some(parent), Some(own)) => Some(parent.min(own)),
                (parent, own) => parent.or(own),
            };
            result.push(json!({
                "depth":depth,"limit_window_seconds":rule.duration_seconds,
                "started_at":start,"reset_at":end,"reset_after_seconds":end.map(|v|v-now),
                "used_percent":limit.map(|limit|if limit==0 {100_i64} else {(i128::from(used)*100/i128::from(limit)).clamp(0,100) as i64}),
                "allowed":effective_remaining.is_none_or(|remaining|remaining>0),
                "used_usd":crate::format_units(used,9),"limit_usd":rule.cost_limit_usd,
                "remaining_usd":remaining.map(|v|crate::format_units(v,9)),
                "effective_remaining_usd":effective_remaining.map(|v|crate::format_units(v,9)),
            }));
            parent_start = start;
            parent_end = end;
            parent_remaining = effective_remaining;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_official_seven_or_thirty_day_period_with_optional_five_hour_window() {
        for outer in [604800, 2592000] {
            assert!(
                spending_windows(&json!([
                    {"duration_seconds": outer, "cost_limit_usd": "10"},
                    {"duration_seconds": 18000, "cost_limit_usd": "2"}
                ]))
                .is_ok()
            );
            assert!(
                spending_windows(&json!([
                    {"duration_seconds": outer, "cost_limit_usd": "10"}
                ]))
                .is_ok()
            );
        }
    }

    #[test]
    fn rejects_extra_layers_and_non_official_durations() {
        assert!(
            spending_windows(&json!([
                {"duration_seconds":604800,"cost_limit_usd":"10"},
                {"duration_seconds":18000,"cost_limit_usd":"2"},
                {"duration_seconds":3600,"cost_limit_usd":"1"}
            ]))
            .is_err()
        );
        assert!(
            spending_windows(&json!([
                {"duration_seconds":2592000,"cost_limit_usd":"10"},
                {"duration_seconds":604800,"cost_limit_usd":"2"}
            ]))
            .is_err()
        );
        assert!(
            spending_windows(&json!([
                {"duration_seconds":604800,"cost_limit_usd":"10"},
                {"duration_seconds":18000,"cost_limit_usd":"-1"}
            ]))
            .is_err()
        );
    }
}

use crate::{Result, Storage, StorageError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Connection;

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

/// Plans use one canonical nested-window representation after schema migration.
pub fn plan_spending_windows(config: &Value) -> Result<Vec<SpendingWindow>> {
    spending_windows(&config["spending_windows"])
}

impl Storage {
    async fn spending_decision(&self, owner: &str) -> Result<std::sync::Arc<SpendingDecision>> {
        let key = format!("spending-decision:{owner}");
        let current = || {
            self.runtime
                .cached::<std::sync::Arc<SpendingDecision>>(&key)
                .filter(|value| value.version == self.runtime.version(&key))
        };
        if let Some(value) = current() {
            return Ok(value);
        }
        let _fill = self.runtime.cache_fill(&key).await;
        if let Some(value) = current() {
            return Ok(value);
        }
        let version = self.runtime.version(&key);
        let epoch = self.runtime.epoch();
        let now = chrono::Utc::now().timestamp();
        let entitlements = self.effective_entitlements(owner).await?;
        let limited = entitlements
            .spending_windows
            .iter()
            .any(|w| w.cost_limit_usd.is_some());
        let windows = if limited {
            let anchor = self.subscription_anchor(owner).await?;
            self.nested_spending_windows(
                owner,
                &entitlements.spending_windows,
                if anchor > now { 0 } else { anchor },
                now,
            )
            .await?
        } else {
            Vec::new()
        };
        let allowed = windows.iter().all(|w| w["allowed"] != false);
        let deadline = windows
            .iter()
            .filter_map(|w| w["reset_at"].as_i64())
            .chain(entitlements.expires_at)
            .filter(|at| *at > now)
            .min();
        let ttl_ms = deadline.map_or(30_000, |at| {
            at.saturating_mul(1000)
                .saturating_sub(chrono::Utc::now().timestamp_millis())
                .clamp(0, 30_000)
        });
        let value = std::sync::Arc::new(SpendingDecision {
            version,
            allowed,
            limited,
            windows,
            plan_type: entitlements.plan_type.clone(),
            enabled: entitlements.execution_enabled,
        });
        self.runtime.cache(
            key,
            &value,
            epoch,
            std::time::Duration::from_millis(ttl_ms as u64),
        );
        Ok(value)
    }

    /// Admission reads one cached predicate, never a collection of usage records.
    pub async fn virtual_spending_status(&self, owner: &str) -> Result<(bool, bool)> {
        let value = self.spending_decision(owner).await?;
        Ok((value.allowed, value.limited))
    }

    pub async fn virtual_spending_limited(&self, owner: &str) -> Result<bool> {
        Ok(self
            .effective_entitlements(owner)
            .await?
            .spending_windows
            .iter()
            .any(|w| w.cost_limit_usd.is_some()))
    }

    /// Protocol quota fields share the admission snapshot; administration owns lifetime totals.
    pub async fn execution_quota(&self, owner: &str) -> Result<Value> {
        let value = self.spending_decision(owner).await?;
        Ok(quota_view(
            owner,
            &value.plan_type,
            value.enabled && value.allowed,
            &value.windows,
            chrono::Utc::now().timestamp(),
        ))
    }

    /// 只读账本推导内层起点，不因打开额度页而启动周期或取得写锁。
    pub(crate) async fn nested_spending_windows(
        &self,
        owner: &str,
        rules: &[SpendingWindow],
        anchor: i64,
        now: i64,
    ) -> Result<Vec<Value>> {
        let mut connection = self.pool().acquire().await?;
        windows_on(&mut connection, owner, rules, anchor, now).await
    }
}

impl crate::runtime::Runtime {
    pub(crate) fn record_spending_use(&self, record: &crate::UsageRecord) {
        let key = format!("spending-decision:{}", record.subject_id);
        let Some(value) = self
            .cached::<std::sync::Arc<SpendingDecision>>(&key)
            .filter(|value| value.version == self.version(&key))
        else {
            // Fence a concurrent cache fill that may have read before this insert.
            self.advance_version(&key);
            return;
        };
        if let Some(inner) = value.windows.get(1) {
            let at = record.requested_at_ms.div_euclid(1000);
            // A new first use or an earlier concurrently inserted timestamp can
            // change the five-hour anchor. Later uses do not change admission.
            if inner["started_at"].as_i64().is_none_or(|start| at < start) {
                self.advance_version(&key);
            }
        }
    }
}

struct SpendingDecision {
    version: u64,
    allowed: bool,
    limited: bool,
    enabled: bool,
    plan_type: String,
    windows: Vec<Value>,
}

pub(crate) fn quota_view(
    owner: &str,
    plan: &str,
    allowed: bool,
    windows: &[Value],
    now: i64,
) -> Value {
    let mut windows = windows.to_vec();
    for window in &mut windows {
        window["reset_after_seconds"] = window["reset_at"]
            .as_i64()
            .map(|at| at.saturating_sub(now))
            .into();
    }
    let inner = windows
        .get(1)
        .filter(|w| !w["started_at"].is_null() && !w["used_percent"].is_null());
    let outer = windows.first().filter(|w| !w["used_percent"].is_null());
    let primary = inner.or(outer).cloned().unwrap_or(Value::Null);
    let secondary = inner.and(outer).cloned().unwrap_or(Value::Null);
    json!({"account_id":owner,"user_id":format!("user-{owner}"),"plan_type":plan,
        "rate_limit":{"allowed":allowed,"limit_reached":!allowed,"primary_window":primary,"secondary_window":secondary,"windows":windows},
        "credits":{"has_credits":false,"unlimited":false,"balance":null}})
}

pub(crate) async fn windows_on(
    connection: &mut sqlx::SqliteConnection,
    owner: &str,
    rules: &[SpendingWindow],
    anchor: i64,
    now: i64,
) -> Result<Vec<Value>> {
    let Some(root) = rules.first() else {
        return Ok(Vec::new());
    };
    // All windows describe one read snapshot, including when called inside a
    // reset-card transaction (sqlx uses a nested savepoint in that case).
    let mut tx = connection.begin().await?;
    // A card restarts the quota clock, independently of subscription expiration.
    // Old in-flight requests retain their original timestamp and stay out of the
    // new windows even if their historical bill settles after redemption.
    let reset: Option<(String,i64)> = sqlx::query_as(
            "SELECT c.id,c.redeemed_at_ms FROM virtual_reset_credits c JOIN platform_accounts a ON a.quota_reset_credit_id=c.id AND a.id=c.virtual_account_id WHERE a.id=? AND c.redeemed_at_ms<?",
        )
        .bind(owner)
        .bind(now.saturating_add(1).saturating_mul(1000))
        .fetch_optional(&mut *tx)
        .await?;
    let reset_at = reset.as_ref().map(|(_, at)| *at);
    let anchor = reset_at.map_or(anchor, |reset| anchor.max(reset.div_euclid(1000)));
    let root_start = anchor
        .checked_add(
            now.saturating_sub(anchor).max(0) / root.duration_seconds * root.duration_seconds,
        )
        .ok_or_else(|| StorageError::Constraint("额度周期超出时间范围".into()))?;
    let from_ms = root_start
        .checked_mul(1000)
        .ok_or_else(|| StorageError::Constraint("额度周期超出时间范围".into()))?;
    let from_ms = reset_at.map_or(from_ms, |reset| from_ms.max(reset));
    let until_ms = now
        .checked_add(1)
        .and_then(|v| v.checked_mul(1000))
        .ok_or_else(|| StorageError::Constraint("额度周期超出时间范围".into()))?;
    let reset_id = reset.as_ref().map(|(id, _)| id.as_str());
    let mut parent_start = Some(root_start);
    let mut parent_end = None;
    let mut parent_remaining: Option<i64> = None;
    let mut result = Vec::with_capacity(rules.len());
    for (depth, rule) in rules.iter().enumerate() {
        let start = if depth == 0 {
            Some(root_start)
        } else if let Some(parent_start) = parent_start {
            // Jump between actual first uses after each expiry. A 30-day outer
            // period contains at most 144 five-hour starts, regardless of row count.
            let first_ms = from_ms.max(parent_start.saturating_mul(1000));
            let mut query = sqlx::QueryBuilder::new(
                "WITH RECURSIVE periods(at) AS (SELECT MIN(requested_at_ms)/1000 FROM usage_records",
            );
            usage_scope(&mut query, owner, first_ms, until_ms, reset_id);
            query.push(" UNION ALL SELECT (SELECT MIN(requested_at_ms)/1000 FROM usage_records");
            usage_scope(&mut query, owner, first_ms, until_ms, reset_id);
            query
                .push(" AND requested_at_ms >= (periods.at + ")
                .push_bind(rule.duration_seconds)
                .push(") * 1000) FROM periods WHERE at IS NOT NULL) SELECT MAX(at) FROM periods");
            let start: Option<i64> = query.build_query_scalar().fetch_one(&mut *tx).await?;
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
        let used: i64 = if let Some(start) = start {
            let mut query =
                sqlx::QueryBuilder::new("SELECT COALESCE(SUM(cost_nano_usd),0) FROM usage_records");
            usage_scope(
                &mut query,
                owner,
                from_ms.max(start.saturating_mul(1000)),
                until_ms,
                reset_id,
            );
            query.build_query_scalar().fetch_one(&mut *tx).await?
        } else {
            0
        };
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
    tx.commit().await?;
    Ok(result)
}

fn usage_scope<'a>(
    query: &mut sqlx::QueryBuilder<'a, sqlx::Sqlite>,
    owner: &'a str,
    from_ms: i64,
    until_ms: i64,
    reset: Option<&'a str>,
) {
    query
        .push(" WHERE subject_id=")
        .push_bind(owner)
        .push(" AND NOT (account_id='' AND status='failed' AND billing_status='not_charged')")
        .push(" AND requested_at_ms>=")
        .push_bind(from_ms)
        .push(" AND requested_at_ms<")
        .push_bind(until_ms);
    if let Some(reset) = reset {
        query.push(" AND quota_reset_credit_id=").push_bind(reset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn rejected_requests_do_not_start_inner_spending_windows() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("rejected-windows.sqlite"))
            .await
            .unwrap();
        let now = chrono::Utc::now().timestamp();
        let record = crate::UsageRecord {
            id: "rejected".into(),
            subject_id: "consumer".into(),
            endpoint: "/v1/responses/compact".into(),
            transport: "http".into(),
            requested_at_ms: now * 1000,
            status: "failed".into(),
            http_status: Some(413),
            error_code: Some("request_too_large".into()),
            ..Default::default()
        };
        storage.insert_rejected_usage(&record).await.unwrap();
        let rules = spending_windows(&json!([
            {"duration_seconds": 604800, "cost_limit_usd":"10"},
            {"duration_seconds": 18000, "cost_limit_usd":"2"}
        ]))
        .unwrap();
        let windows = storage
            .nested_spending_windows("consumer", &rules, now, now + 1)
            .await
            .unwrap();
        assert!(windows[1]["started_at"].is_null());
        let actual = crate::UsageRecord {
            id: "execution".into(),
            account_id: "supplier".into(),
            status: "in_progress".into(),
            http_status: None,
            error_code: None,
            ..record
        };
        storage.insert_usage(&actual).await.unwrap();
        let windows = storage
            .nested_spending_windows("consumer", &rules, now, now + 1)
            .await
            .unwrap();
        assert_eq!(windows[1]["started_at"], now);
        storage.close().await;
    }

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

use crate::{Result, Storage, UsageFilter, usage::conditions};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageGroup {
    Hour,
    #[default]
    Day,
    Model,
    VirtualAccount,
}

#[derive(Debug, FromRow, Serialize)]
pub struct UsageTotals {
    pub request_count: i64,
    pub completed_requests: i64,
    pub failed_requests: i64,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub cost_nano_usd: Option<i64>,
    pub cache_rate: Option<f64>,
    pub missing_token_requests: i64,
    pub missing_cache_requests: i64,
    pub unpriced_requests: i64,
}

#[derive(Debug, FromRow, Serialize)]
pub struct UsageStatisticsRow {
    pub key: String,
    pub label: String,
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub totals: UsageTotals,
}

#[derive(Debug, FromRow, Serialize)]
pub struct ModelUsageBucket {
    pub bucket: String,
    pub model: String,
    pub total_tokens: Option<i64>,
    pub missing_token_requests: i64,
}

#[derive(Serialize)]
pub struct UsageStatistics {
    pub group_by: UsageGroup,
    pub from_ms: i64,
    pub until_ms: i64,
    pub tz_offset: i32,
    pub summary: UsageTotals,
    pub rows: Vec<UsageStatisticsRow>,
    pub model_usage: Vec<ModelUsageBucket>,
}

// Aggregate the ledger, never the current record page or current model prices.
// Cache and reasoning are subsets of input/output. NULLs remain unknown; when
// some values are known their subtotal is returned with explicit missing counts.
// Cache rate divides those reported subtotals; missing rows do not hide the ratio.
pub(crate) const TOTALS: &str = "COUNT(*) AS request_count,
    COALESCE(SUM(status IN ('completed','client_stopped')),0) AS completed_requests,
    COALESCE(SUM(status IN ('failed','incomplete','interrupted')),0) AS failed_requests,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(input_tokens) END AS input_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(output_tokens) END AS output_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(CASE
        WHEN input_tokens IS NOT NULL OR output_tokens IS NOT NULL
        THEN COALESCE(input_tokens,0)+COALESCE(output_tokens,0) END) END AS total_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(cached_tokens) END AS cached_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(cache_write_tokens) END AS cache_write_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(reasoning_tokens) END AS reasoning_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(cost_nano_usd) END AS cost_nano_usd,
    CASE WHEN SUM(input_tokens)>0 THEN 100.0*SUM(cached_tokens)/SUM(input_tokens) END AS cache_rate,
    COALESCE(SUM(input_tokens IS NULL OR output_tokens IS NULL),0) AS missing_token_requests,
    COALESCE(SUM(input_tokens IS NULL OR (input_tokens>0 AND cached_tokens IS NULL)),0) AS missing_cache_requests,
    COALESCE(SUM(cost_nano_usd IS NULL),0) AS unpriced_requests";

impl Storage {
    pub async fn usage_statistics(
        &self,
        filter: &UsageFilter,
        group_by: UsageGroup,
        tz_offset: i32,
    ) -> Result<UsageStatistics> {
        let (Some(from_ms), Some(until_ms)) = (filter.from_ms, filter.until_ms) else {
            return Err(crate::StorageError::Constraint("请选择统计时间范围".into()));
        };
        let span = until_ms.checked_sub(from_ms).unwrap_or(0);
        let max_days = if matches!(group_by, UsageGroup::Hour) {
            31
        } else {
            366
        };
        if !(-840..=840).contains(&tz_offset) || span <= 0 || span > max_days * 86_400_000 {
            return Err(crate::StorageError::Constraint(format!(
                "统计时间或时区无效，当前分组最多查询 {max_days} 天"
            )));
        }
        let local_time = format!(
            "(requested_at_ms - {}) / 1000.0",
            i64::from(tz_offset) * 60_000
        );
        let key = match group_by {
            UsageGroup::Hour => format!("strftime('%Y-%m-%dT%H:00', {local_time}, 'unixepoch')"),
            UsageGroup::Day => format!("strftime('%Y-%m-%d', {local_time}, 'unixepoch')"),
            UsageGroup::Model => "COALESCE(NULLIF(actual_model,''),NULLIF(model,''),'')".into(),
            UsageGroup::VirtualAccount => "subject_id".into(),
        };
        let label = match group_by {
            UsageGroup::VirtualAccount => {
                "COALESCE((SELECT username FROM virtual_principals WHERE id=subject_id),NULLIF(MAX(subject_name),''),subject_id)"
            }
            _ => &key,
        };
        let mut tx = self.pool().begin().await?;
        let mut summary = QueryBuilder::new(format!("SELECT {TOTALS} FROM usage_records"));
        conditions(&mut summary, filter);
        summary.push(" AND subject_kind='virtual_account'");
        let summary = summary
            .build_query_as::<UsageTotals>()
            .fetch_one(&mut *tx)
            .await?;
        let mut rows = QueryBuilder::new(format!(
            "SELECT {key} AS key, {label} AS label, {TOTALS} FROM usage_records"
        ));
        conditions(&mut rows, filter);
        rows.push(" AND subject_kind='virtual_account' GROUP BY ")
            .push(&key)
            .push(" ORDER BY key");
        let rows = rows
            .build_query_as::<UsageStatisticsRow>()
            .fetch_all(&mut *tx)
            .await?;
        let model_key = "COALESCE(NULLIF(actual_model,''),NULLIF(model,''),'')";
        let mut model_usage = QueryBuilder::new(format!(
            "SELECT {key} AS bucket,
             {model_key} AS model,
             SUM(CASE WHEN input_tokens IS NOT NULL OR output_tokens IS NOT NULL
                 THEN COALESCE(input_tokens,0)+COALESCE(output_tokens,0) END) AS total_tokens,
             SUM(input_tokens IS NULL OR output_tokens IS NULL) AS missing_token_requests
             FROM usage_records"
        ));
        conditions(&mut model_usage, filter);
        model_usage
            .push(" AND subject_kind='virtual_account' GROUP BY bucket, ")
            .push(model_key)
            .push(" ORDER BY bucket, model");
        let model_usage = model_usage
            .build_query_as::<ModelUsageBucket>()
            .fetch_all(&mut *tx)
            .await?;
        tx.commit().await?;
        // Empty time intervals are filled by the UI using the returned range;
        // an interval containing unknown usage remains a real row with NULLs.
        Ok(UsageStatistics {
            group_by,
            from_ms,
            until_ms,
            tz_offset,
            summary,
            rows,
            model_usage,
        })
    }
}

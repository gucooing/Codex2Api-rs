use crate::{Result, Storage, UsageFilter, usage::conditions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, QueryBuilder};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageGroup {
    Hour,
    #[default]
    Day,
    Model,
    Subject,
}

#[derive(Clone, Debug, FromRow, Deserialize, Serialize)]
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

#[derive(Clone, Debug, FromRow, Deserialize, Serialize)]
pub struct UsageStatisticsRow {
    pub key: String,
    pub label: String,
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub totals: UsageTotals,
}

#[derive(Clone, Debug, FromRow, Deserialize, Serialize)]
pub struct ModelUsageBucket {
    pub bucket: String,
    pub model: String,
    pub total_tokens: Option<i64>,
    pub missing_token_requests: i64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct UsageStatistics {
    pub group_by: UsageGroup,
    pub from_ms: i64,
    pub until_ms: i64,
    pub tz_offset: i32,
    pub summary: UsageTotals,
    pub rows: Vec<UsageStatisticsRow>,
    pub model_usage: Vec<ModelUsageBucket>,
}

#[derive(FromRow)]
struct AggregateRow {
    level: i32,
    key: Option<String>,
    model: Option<String>,
    label: Option<String>,
    #[sqlx(flatten)]
    totals: UsageTotals,
}

pub(crate) const STATISTICS_TTL: Duration = Duration::from_secs(15);

pub(crate) fn time_bucket_sql(duration_ms: i64, tz_offset: i32) -> String {
    let local = format!("(requested_at_ms - ({}))", i64::from(tz_offset) * 60_000);
    // Integer division truncates toward zero; the remainder correction also covers pre-epoch data.
    format!("({local}/{duration_ms} - CASE WHEN {local}%{duration_ms}<0 THEN 1 ELSE 0 END)")
}

pub(crate) fn statistics_cache_key(family: &str, filters: impl Serialize) -> Result<String> {
    Ok(format!(
        "{family}:{:x}",
        Sha256::digest(serde_json::to_vec(&filters)?)
    ))
}

// Aggregate the ledger, never the current record page or current model prices.
// Cache and reasoning are subsets of input/output. NULLs remain unknown; when
// some values are known their subtotal is returned with explicit missing counts.
// Cache rate divides those reported subtotals; missing rows do not hide the ratio.
pub(crate) const TOTALS: &str = "COUNT(*) AS request_count,
    COALESCE(SUM((status IN ('completed','client_stopped'))::int)::bigint,0) AS completed_requests,
    COALESCE(SUM((status IN ('failed','incomplete','interrupted'))::int)::bigint,0) AS failed_requests,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(input_tokens)::bigint END AS input_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(output_tokens)::bigint END AS output_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(CASE
        WHEN input_tokens IS NOT NULL OR output_tokens IS NOT NULL
        THEN COALESCE(input_tokens,0)+COALESCE(output_tokens,0) END)::bigint END AS total_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(cached_tokens)::bigint END AS cached_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(cache_write_tokens)::bigint END AS cache_write_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(reasoning_tokens)::bigint END AS reasoning_tokens,
    CASE WHEN COUNT(*)=0 THEN 0 ELSE SUM(cost_nano_usd)::bigint END AS cost_nano_usd,
    CASE WHEN SUM(input_tokens)::bigint>0 THEN 100.0::double precision*SUM(cached_tokens)::bigint/SUM(input_tokens)::bigint END AS cache_rate,
    COALESCE(SUM((input_tokens IS NULL OR output_tokens IS NULL)::int)::bigint,0) AS missing_token_requests,
    COALESCE(SUM((input_tokens IS NULL OR (input_tokens>0 AND cached_tokens IS NULL))::int)::bigint,0) AS missing_cache_requests,
    COALESCE(SUM((cost_nano_usd IS NULL)::int)::bigint,0) AS unpriced_requests";

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
        let mut cache_filter = filter.clone();
        cache_filter.page = 0;
        cache_filter.page_size = None;
        let cache_key =
            statistics_cache_key("usage-statistics", (&cache_filter, group_by, tz_offset))?;
        self.runtime
            .snapshots
            .read(cache_key, async {
                let statistics = self
                    .load_usage_statistics(filter, group_by, tz_offset, from_ms, until_ms)
                    .await?;
                Ok((statistics, STATISTICS_TTL))
            })
            .await
    }

    async fn load_usage_statistics(
        &self,
        filter: &UsageFilter,
        group_by: UsageGroup,
        tz_offset: i32,
        from_ms: i64,
        until_ms: i64,
    ) -> Result<UsageStatistics> {
        let (key, display_key) = match group_by {
            UsageGroup::Hour => (
                time_bucket_sql(3_600_000, tz_offset),
                "to_char(to_timestamp(bucket*3600.0::double precision) AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:00')",
            ),
            UsageGroup::Day => (
                time_bucket_sql(86_400_000, tz_offset),
                "to_char(to_timestamp(bucket*86400.0::double precision) AT TIME ZONE 'UTC','YYYY-MM-DD')",
            ),
            UsageGroup::Model => (
                "COALESCE(NULLIF(actual_model,''),NULLIF(model,''),'')".into(),
                "bucket",
            ),
            UsageGroup::Subject => ("subject_id".into(), "bucket"),
        };
        let label = match group_by {
            UsageGroup::Subject => {
                "COALESCE((SELECT username FROM platform_principals WHERE id=bucket),NULLIF(MAX(subject_name),''),bucket)"
            }
            _ => display_key,
        };
        let model_key = "COALESCE(NULLIF(actual_model,''),NULLIF(model,''),'')";
        // One ledger scan and one MVCC snapshot for the summary, chart and model stacks.
        let mut query = QueryBuilder::new(format!(
            "SELECT GROUPING(bucket,chart_model) AS level,{display_key} AS key,chart_model AS model,
             {label} AS label,{TOTALS} FROM (
             SELECT {key} AS bucket,{model_key} AS chart_model,subject_name,status,
             input_tokens,output_tokens,cached_tokens,cache_write_tokens,reasoning_tokens,cost_nano_usd
             FROM usage_records"
        ));
        conditions(&mut query, filter);
        query.push(") AS ledger GROUP BY GROUPING SETS ((bucket,chart_model),(bucket),()) ORDER BY bucket,chart_model");
        let aggregates = query
            .build_query_as::<AggregateRow>()
            .fetch_all(self.pool())
            .await?;
        let mut summary = None;
        let mut rows = Vec::new();
        let mut model_usage = Vec::new();
        for row in aggregates {
            match row.level {
                0 => model_usage.push(ModelUsageBucket {
                    bucket: row.key.unwrap_or_default(),
                    model: row.model.unwrap_or_default(),
                    total_tokens: row.totals.total_tokens,
                    missing_token_requests: row.totals.missing_token_requests,
                }),
                1 => rows.push(UsageStatisticsRow {
                    key: row.key.unwrap_or_default(),
                    label: row.label.unwrap_or_default(),
                    totals: row.totals,
                }),
                _ => summary = Some(row.totals),
            }
        }
        let summary = summary.ok_or(sqlx::Error::RowNotFound)?;
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

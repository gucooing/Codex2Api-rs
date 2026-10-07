//! User-owned ledger queries. No supplier joins, raw errors or account selectors.
use crate::{Result, Storage, StorageError, UsageStatisticsRow, UsageTotals};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, QueryBuilder, Sqlite};

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UserUsageFilter {
    pub days: i64,
    pub tz_offset: i32,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub page: i64,
    pub limit: i64,
}
impl Default for UserUsageFilter {
    fn default() -> Self {
        Self {
            days: 7,
            tz_offset: 0,
            provider: String::new(),
            model: String::new(),
            status: String::new(),
            page: 1,
            limit: 20,
        }
    }
}

#[derive(FromRow, Serialize)]
struct UserUsageRecord {
    id: String,
    provider_id: String,
    model: Option<String>,
    actual_model: Option<String>,
    requested_at_ms: i64,
    status: String,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cached_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    total_ms: Option<i64>,
    first_byte_ms: Option<i64>,
    cost_nano_usd: Option<i64>,
    billing_status: String,
}

fn scope(
    query: &mut QueryBuilder<'_, Sqlite>,
    owner: &str,
    filter: &UserUsageFilter,
    from: i64,
    until: i64,
) {
    query.push(" WHERE subject_kind='virtual_account' AND subject_id IN (SELECT id FROM regular_platforms WHERE user_id=").push_bind(owner.to_owned()).push(") AND requested_at_ms>=").push_bind(from).push(" AND requested_at_ms<").push_bind(until);
    if !filter.provider.is_empty() {
        query
            .push(" AND provider_id=")
            .push_bind(filter.provider.clone());
    }
    if !filter.model.is_empty() {
        query
            .push(" AND instr(lower(COALESCE(NULLIF(actual_model,''),model,'')),lower(")
            .push_bind(filter.model.trim().to_owned())
            .push("))>0");
    }
    if filter.status == "failed" {
        query.push(" AND status IN ('failed','interrupted','incomplete')");
    }
    if filter.status == "completed" {
        query.push(" AND status IN ('completed','client_stopped')");
    }
}

impl Storage {
    pub(crate) async fn user_usage(&self, owner: &str, filter: &UserUsageFilter) -> Result<Value> {
        if ![1, 7, 30].contains(&filter.days)
            || !(-840..=840).contains(&filter.tz_offset)
            || filter.provider.len() > 64
            || filter.model.len() > 100
            || !["", "completed", "failed"].contains(&filter.status.as_str())
            || filter.page < 1
            || ![10, 20, 30, 50].contains(&filter.limit)
        {
            return Err(StorageError::InvalidAdminUpdate("使用记录筛选条件无效"));
        }
        let offset = (filter.page - 1)
            .checked_mul(filter.limit)
            .ok_or(StorageError::InvalidAdminUpdate("页码超出范围"))?;
        let until = chrono::Utc::now().timestamp_millis() + 1;
        let zone_ms = i64::from(filter.tz_offset) * 60_000;
        let from = (until - 1 - zone_ms).div_euclid(86_400_000) * 86_400_000 + zone_ms
            - (filter.days - 1) * 86_400_000;
        let totals = crate::usage_statistics::TOTALS;
        let mut tx = self.pool().begin().await?;
        crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        let mut summary = QueryBuilder::new(format!("SELECT {totals} FROM usage_records"));
        scope(&mut summary, owner, filter, from, until);
        let summary = summary
            .build_query_as::<UsageTotals>()
            .fetch_one(&mut *tx)
            .await?;
        let bucket =
            format!("strftime('%Y-%m-%d',(requested_at_ms - ({zone_ms}))/1000.0,'unixepoch')");
        let mut series = QueryBuilder::new(format!(
            "SELECT {bucket} AS key,{bucket} AS label,{totals} FROM usage_records"
        ));
        scope(&mut series, owner, filter, from, until);
        series.push(" GROUP BY key ORDER BY key");
        let rows = series
            .build_query_as::<UsageStatisticsRow>()
            .fetch_all(&mut *tx)
            .await?;
        let mut records = QueryBuilder::new(
            "SELECT id,provider_id,model,actual_model,requested_at_ms,status,input_tokens,output_tokens,cached_tokens,cache_write_tokens,reasoning_tokens,total_ms,first_byte_ms,cost_nano_usd,billing_status FROM usage_records",
        );
        scope(&mut records, owner, filter, from, until);
        records
            .push(" ORDER BY requested_at_ms DESC,id DESC LIMIT ")
            .push_bind(filter.limit)
            .push(" OFFSET ")
            .push_bind(offset);
        let items = records
            .build_query_as::<UserUsageRecord>()
            .fetch_all(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(
            json!({"from_ms":from,"until_ms":until,"tz_offset":filter.tz_offset,"summary":summary,"rows":rows,"items":items,"total":summary.request_count,"page":filter.page,"limit":filter.limit}),
        )
    }
}

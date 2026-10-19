//! User-owned ledger queries. No supplier joins, raw errors or account selectors.
use crate::{Result, Storage, StorageError, UsageStatisticsRow, UsageTotals};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Postgres, QueryBuilder};

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UserUsageFilter {
    pub source: Option<crate::usage::UsageSource>,
    pub days: i64,
    pub tz_offset: i32,
    pub provider: String,
    pub model: String,
    pub key_id: String,
    pub group_id: String,
    pub from_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub status: String,
    pub page: i64,
    pub limit: i64,
}
impl Default for UserUsageFilter {
    fn default() -> Self {
        Self {
            source: None,
            days: 7,
            tz_offset: 0,
            provider: String::new(),
            model: String::new(),
            key_id: String::new(),
            group_id: String::new(),
            from_ms: None,
            until_ms: None,
            status: String::new(),
            page: 1,
            limit: 20,
        }
    }
}

#[derive(FromRow, Serialize)]
struct UserUsageRecord {
    id: String,
    consumer_name: String,
    subject_kind: String,
    reasoning_effort: Option<String>,
    service_tier: Option<String>,
    image_usage_json: Option<String>,
    image_input_usage_json: Option<String>,
    key_id: Option<String>,
    source_name: Option<String>,
    endpoint: String,
    upstream_endpoint: Option<String>,
    client_ip: Option<String>,
    transport: String,
    stream: Option<bool>,
    /// The failure the consumer received, never the supplier's real error.
    error_code: Option<String>,
    error_reason: Option<String>,
    multiplier_micros: i64,
    wallet_debit_cents: i64,
    image_count: Option<i64>,
    image_size: Option<String>,
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
    first_token_ms: Option<i64>,
    cost_nano_usd: Option<i64>,
    billing_status: String,
    #[serde(skip_serializing)]
    pricing_snapshot_json: Option<String>,
    #[sqlx(skip)]
    billing_calculation: Option<crate::billing::UsageCalculation>,
}

impl UserUsageRecord {
    fn calculate(&mut self) {
        let Some(snapshot) = self
            .pricing_snapshot_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<crate::billing::BillingSnapshot>(json).ok())
        else {
            return;
        };
        let record = crate::UsageRecord {
            provider_id: self.provider_id.clone(),
            endpoint: self.endpoint.clone(),
            model: self.model.clone(),
            actual_model: self.actual_model.clone(),
            service_tier: self.service_tier.clone(),
            status: self.status.clone(),
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cached_tokens: self.cached_tokens,
            cache_write_tokens: self.cache_write_tokens,
            image_count: self.image_count,
            image_usage_json: self.image_usage_json.clone(),
            cost_nano_usd: self.cost_nano_usd,
            multiplier_micros: self.multiplier_micros,
            billing_status: self.billing_status.clone(),
            ..Default::default()
        };
        self.billing_calculation = snapshot.calculation(&record, false);
    }
}

fn scope(
    query: &mut QueryBuilder<'_, Postgres>,
    owner: &str,
    filter: &UserUsageFilter,
    from: i64,
    until: i64,
) {
    query
        .push(" WHERE user_id=")
        .push_bind(owner.to_owned())
        .push(" AND requested_at_ms>=")
        .push_bind(from)
        .push(" AND requested_at_ms<")
        .push_bind(until);
    match filter.source {
        Some(crate::usage::UsageSource::Api) => {
            query.push(" AND subject_kind='api_key'");
        }
        Some(crate::usage::UsageSource::Subscription) => {
            query.push(" AND subject_kind='virtual_account'");
        }
        None => {}
    }
    for (column, value) in [("key_id", &filter.key_id), ("group_id", &filter.group_id)] {
        if !value.is_empty() {
            query
                .push(" AND ")
                .push(column)
                .push("=")
                .push_bind(value.clone());
        }
    }
    if !filter.provider.is_empty() {
        query
            .push(" AND provider_id=")
            .push_bind(filter.provider.clone());
    }
    if !filter.model.is_empty() {
        query
            .push(" AND strpos(lower(COALESCE(NULLIF(actual_model,''),model,'')),lower(")
            .push_bind(filter.model.trim().to_owned())
            .push("))>0");
    }
    if filter.status == "failed" {
        query.push(" AND status IN ('failed','interrupted','incomplete')");
    }
    if filter.status == "completed" {
        query.push(" AND status IN ('completed','client_stopped')");
    } else if !filter.status.is_empty() && filter.status != "failed" {
        query.push(" AND status=").push_bind(filter.status.clone());
    }
}

impl Storage {
    pub(crate) async fn user_usage(&self, owner: &str, filter: &UserUsageFilter) -> Result<Value> {
        if ![1, 7, 30].contains(&filter.days)
            || !(-840..=840).contains(&filter.tz_offset)
            || filter.provider.len() > 64
            || filter.model.len() > 100
            || filter.key_id.len() > 128
            || filter.group_id.len() > 128
            || ![
                "",
                "in_progress",
                "completed",
                "failed",
                "interrupted",
                "incomplete",
            ]
            .contains(&filter.status.as_str())
            || filter.page < 1
            || ![10, 20, 30, 50].contains(&filter.limit)
        {
            return Err(StorageError::InvalidAdminUpdate("使用记录筛选条件无效"));
        }
        let until = filter
            .until_ms
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() + 1);
        if !(0..=253402300799999).contains(&until)
            || filter
                .from_ms
                .is_some_and(|from| !(0..=253402300799999).contains(&from))
        {
            return Err(StorageError::InvalidAdminUpdate("使用记录时间范围无效"));
        }
        let zone_ms = i64::from(filter.tz_offset) * 60_000;
        let from = filter.from_ms.unwrap_or_else(|| {
            (until.saturating_sub(1).saturating_sub(zone_ms)).div_euclid(86_400_000) * 86_400_000
                + zone_ms
                - (filter.days - 1) * 86_400_000
        });
        if until
            .checked_sub(from)
            .is_none_or(|span| span <= 0 || span > 366 * 86_400_000)
        {
            return Err(StorageError::InvalidAdminUpdate("使用记录时间范围无效"));
        }
        let totals = crate::usage_statistics::TOTALS;
        let mut tx = self
            .pool()
            .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .await?;
        crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::User).await?;
        let mut summary = QueryBuilder::new(format!("SELECT {totals} FROM usage_records"));
        scope(&mut summary, owner, filter, from, until);
        let summary = summary
            .build_query_as::<UsageTotals>()
            .fetch_one(&mut *tx)
            .await?;
        let page = filter
            .page
            .min(((summary.request_count.saturating_sub(1) / filter.limit) + 1).max(1));
        let offset = (page - 1) * filter.limit;
        let bucket = format!(
            "to_char(to_timestamp(((requested_at_ms - ({zone_ms}))/1000.0)::double precision) AT TIME ZONE 'UTC','YYYY-MM-DD')"
        );
        let mut series = QueryBuilder::new(format!(
            "SELECT {bucket} AS key,{bucket} AS label,{totals} FROM usage_records"
        ));
        scope(&mut series, owner, filter, from, until);
        series.push(" GROUP BY key ORDER BY key");
        let rows = series
            .build_query_as::<UsageStatisticsRow>()
            .fetch_all(&mut *tx)
            .await?;
        let mut records = QueryBuilder::new(format!(
            "SELECT id,{} AS consumer_name,subject_kind,reasoning_effort,service_tier,image_usage_json,image_input_usage_json,key_id,source_name,endpoint,upstream_endpoint,client_ip,transport,stream,public_error_code AS error_code,public_error_message AS error_reason,multiplier_micros,wallet_debit_cents,image_count,image_size,provider_id,model,actual_model,requested_at_ms,status,input_tokens,output_tokens,cached_tokens,cache_write_tokens,reasoning_tokens,total_ms,first_token_ms,cost_nano_usd,billing_status,pricing_snapshot_json FROM usage_records",
            crate::usage::CONSUMER_NAME,
        ));
        scope(&mut records, owner, filter, from, until);
        records
            .push(" ORDER BY requested_at_ms DESC,id DESC LIMIT ")
            .push_bind(filter.limit)
            .push(" OFFSET ")
            .push_bind(offset);
        let mut items = records
            .build_query_as::<UserUsageRecord>()
            .fetch_all(&mut *tx)
            .await?;
        let pending: i64 = sqlx::query_scalar(
            "SELECT COALESCE((SELECT pending_nano_usd FROM usage_wallet_state WHERE user_id=$1),0)",
        )
        .bind(owner)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        for item in &mut items {
            item.calculate();
        }
        Ok(
            json!({"wallet_pending_nano_usd":pending,"from_ms":from,"until_ms":until,"tz_offset":filter.tz_offset,"summary":summary,"rows":rows,"items":items,"total":summary.request_count,"page":page,"limit":filter.limit}),
        )
    }
}

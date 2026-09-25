use crate::{Result, Storage};
use sqlx::{FromRow, QueryBuilder, Sqlite};

#[derive(Clone, Debug, FromRow, serde::Serialize)]
pub struct UsageRecord {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub subject_id: String,
    pub subject_name: String,
    pub subject_kind: String,
    pub provider_id: String,
    pub endpoint: String,
    pub transport: String,
    pub model: Option<String>,
    pub actual_model: Option<String>,
    pub upstream_request_id: Option<String>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub image_size: Option<String>,
    pub image_count: Option<i64>,
    pub image_usage_json: Option<String>,
    pub first_byte_ms: Option<i64>,
    pub total_ms: Option<i64>,
    pub requested_at_ms: i64,
    pub status: String,
    pub http_status: Option<i64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub cost_nano_usd: Option<i64>,
    pub billing_status: String,
    pub billing_model: Option<String>,
}
impl Default for UsageRecord {
    fn default() -> Self {
        Self {
            id: Default::default(),
            account_id: Default::default(),
            account_name: Default::default(),
            subject_id: Default::default(),
            subject_name: Default::default(),
            subject_kind: "virtual_account".into(),
            provider_id: codex2api_core::default_provider(),
            endpoint: Default::default(),
            transport: Default::default(),
            model: Default::default(),
            actual_model: Default::default(),
            upstream_request_id: None,
            reasoning_effort: Default::default(),
            service_tier: Default::default(),
            input_tokens: Default::default(),
            output_tokens: Default::default(),
            cached_tokens: Default::default(),
            cache_write_tokens: Default::default(),
            reasoning_tokens: Default::default(),
            image_size: Default::default(),
            image_count: Default::default(),
            image_usage_json: Default::default(),
            first_byte_ms: Default::default(),
            total_ms: Default::default(),
            requested_at_ms: Default::default(),
            status: Default::default(),
            http_status: Default::default(),
            error_code: None,
            error_message: None,
            cost_nano_usd: Default::default(),
            billing_status: Default::default(),
            billing_model: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct UsageFilter {
    pub account: Option<String>,
    pub account_id: Option<String>,
    pub subject_id: Option<String>,
    pub model: Option<String>,
    pub from_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub status: Option<String>,
    pub page: u32,
    pub page_size: Option<u32>,
}

pub const USAGE_PAGE_SIZE: i64 = 20;

pub fn table_page_size(size: Option<u32>) -> Result<i64> {
    match size {
        None => Ok(USAGE_PAGE_SIZE),
        Some(size @ (10 | 20 | 30 | 50)) => Ok(i64::from(size)),
        _ => Err(crate::StorageError::Constraint(
            "每页条数只能为 10、20、30 或 50".into(),
        )),
    }
}
#[derive(serde::Serialize)]
pub struct UsagePage {
    pub records: Vec<UsageRecord>,
    pub total: i64,
    pub page: u32,
    pub page_size: i64,
}

#[derive(serde::Serialize)]
pub struct AccountUsageSummary {
    pub lifetime_tokens: Option<i64>,
    pub peak_daily_tokens: Option<i64>,
    pub current_streak_days: u64,
    pub longest_streak_days: u64,
    pub longest_running_turn_sec: Option<i64>,
    pub daily_usage_buckets: Vec<AccountDailyUsage>,
}

#[derive(FromRow, serde::Serialize)]
pub struct AccountDailyUsage {
    pub start_date: chrono::NaiveDate,
    pub tokens: Option<i64>,
    #[serde(skip)]
    pub longest_running_turn_ms: Option<i64>,
}

#[derive(FromRow)]
pub struct VirtualDailyModelTokens {
    pub date: chrono::NaiveDate,
    pub model: String,
    pub tokens: i64,
}

fn conditions(query: &mut QueryBuilder<'_, Sqlite>, filter: &UsageFilter) {
    query.push(" WHERE 1=1");
    if let Some(id) = filter.account_id.as_deref().filter(|s| !s.is_empty()) {
        query.push(" AND account_id = ").push_bind(id.to_string());
    }
    if let Some(account) = filter.account.as_deref().filter(|s| !s.is_empty()) {
        query
            .push(" AND instr(lower(account_name), lower(")
            .push_bind(account.to_string())
            .push(")) > 0");
    }
    if let Some(key) = filter.subject_id.as_deref().filter(|s| !s.is_empty()) {
        query.push(" AND subject_id = ").push_bind(key.to_string());
    }
    if let Some(model) = filter.model.as_deref().filter(|s| !s.is_empty()) {
        query
            .push(" AND (instr(lower(model), lower(")
            .push_bind(model.to_string())
            .push(")) > 0 OR instr(lower(actual_model), lower(")
            .push_bind(model.to_string())
            .push(")) > 0)");
    }
    if let Some(status) = filter.status.as_deref().filter(|s| !s.is_empty()) {
        if status == "failed" {
            query.push(" AND status IN ('failed', 'incomplete', 'interrupted')");
        } else if status == "completed" {
            query.push(" AND status IN ('completed', 'client_stopped')");
        } else {
            query.push(" AND status = ").push_bind(status.to_string());
        }
    }
    if let Some(from) = filter.from_ms {
        query.push(" AND requested_at_ms >= ").push_bind(from);
    }
    if let Some(until) = filter.until_ms {
        query.push(" AND requested_at_ms < ").push_bind(until);
    }
}

fn account_usage_summary(
    days: Vec<AccountDailyUsage>,
    today: chrono::NaiveDate,
) -> AccountUsageSummary {
    let mut result = AccountUsageSummary {
        lifetime_tokens: if days.is_empty() {
            Some(0)
        } else {
            days.iter()
                .filter_map(|day| day.tokens)
                .reduce(|a, b| a + b)
        },
        peak_daily_tokens: days
            .iter()
            .filter_map(|day| day.tokens)
            .max()
            .or_else(|| days.is_empty().then_some(0)),
        current_streak_days: 0,
        longest_streak_days: 0,
        longest_running_turn_sec: days
            .iter()
            .filter_map(|day| day.longest_running_turn_ms)
            .max()
            .map(|ms| ms / 1000),
        daily_usage_buckets: Vec::new(),
    };
    let mut previous: Option<chrono::NaiveDate> = None;
    let mut streak = 0;
    for day in days {
        let consecutive = previous.and_then(|date| date.succ_opt()) == Some(day.start_date);
        streak = if consecutive { streak + 1 } else { 1 };
        result.longest_streak_days = result.longest_streak_days.max(streak);
        if day.start_date == today || Some(day.start_date) == today.pred_opt() {
            result.current_streak_days = streak;
        }
        // Days with no requests are zero; a request without reported tokens remains unknown.
        if let Some(mut date) = previous.and_then(|date| date.succ_opt()) {
            while date < day.start_date {
                result.daily_usage_buckets.push(AccountDailyUsage {
                    start_date: date,
                    tokens: Some(0),
                    longest_running_turn_ms: None,
                });
                date = date.succ_opt().expect("date precedes a valid later date");
            }
        }
        previous = Some(day.start_date);
        result.daily_usage_buckets.push(day);
    }
    result
}

impl Storage {
    /// Profile statistics from reported consumption belonging to a virtual identity.
    pub async fn virtual_usage_summary(&self, id: &str) -> Result<AccountUsageSummary> {
        let days: Vec<AccountDailyUsage> = sqlx::query_as(
            "SELECT date(requested_at_ms / 1000, 'unixepoch') AS start_date,
             SUM(COALESCE(input_tokens,0)+COALESCE(output_tokens,0)) AS tokens,
             NULL AS longest_running_turn_ms
             FROM usage_records WHERE subject_id=?
             AND (input_tokens IS NOT NULL OR output_tokens IS NOT NULL)
             GROUP BY start_date ORDER BY start_date",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await?;
        Ok(account_usage_summary(days, chrono::Utc::now().date_naive()))
    }

    /// Actual reported tokens for this virtual identity, across all supplier bindings.
    pub async fn virtual_daily_model_tokens(
        &self,
        id: &str,
        from_ms: i64,
        until_ms: i64,
    ) -> Result<Vec<VirtualDailyModelTokens>> {
        Ok(sqlx::query_as(
            "SELECT date(requested_at_ms / 1000, 'unixepoch') AS date,
             COALESCE(NULLIF(actual_model,''),NULLIF(model,''),'other') AS model,
             SUM(COALESCE(input_tokens,0)+COALESCE(output_tokens,0)) AS tokens
             FROM usage_records WHERE subject_id=? AND requested_at_ms>=? AND requested_at_ms<?
             AND (input_tokens IS NOT NULL OR output_tokens IS NOT NULL)
             GROUP BY date, COALESCE(NULLIF(actual_model,''),NULLIF(model,''),'other')
             ORDER BY date, model",
        )
        .bind(id)
        .bind(from_ms)
        .bind(until_ms)
        .fetch_all(self.pool())
        .await?)
    }

    /// SupplierAccount-wide local usage, grouped by UTC date without splitting request sources.
    pub async fn account_usage_summary(&self, id: &str) -> Result<AccountUsageSummary> {
        let days: Vec<AccountDailyUsage> = sqlx::query_as(
            "SELECT date(requested_at_ms / 1000, 'unixepoch') AS start_date,
            SUM(CASE WHEN input_tokens IS NOT NULL OR output_tokens IS NOT NULL
                THEN COALESCE(input_tokens,0)+COALESCE(output_tokens,0) END) AS tokens,
            MAX(total_ms) AS longest_running_turn_ms
            FROM usage_records WHERE account_id=? GROUP BY start_date ORDER BY start_date",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await?;
        Ok(account_usage_summary(days, chrono::Utc::now().date_naive()))
    }
    /// Called once by the process at startup; a prior process cannot still own these requests.
    pub async fn recover_interrupted_usage(&self) -> Result<()> {
        sqlx::query("UPDATE usage_records SET status='interrupted',error_code=COALESCE(error_code,'process_restarted'),error_message=COALESCE(error_message,'服务重启，请求未完成'),billing_status=CASE WHEN billing_status='pending' THEN 'missing_usage' ELSE billing_status END WHERE status='in_progress'")
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn insert_usage(&self, record: &UsageRecord) -> Result<()> {
        let snapshot = self.billing_snapshot(&record.provider_id).await?;
        sqlx::query("INSERT INTO usage_records (provider_id,subject_kind,id,account_id,account_name,subject_id,subject_name,endpoint,transport,model,reasoning_effort,service_tier,image_size,requested_at_ms,status,actual_model,pricing_snapshot_json,billing_status,error_code,error_message,upstream_request_id,quota_reset_credit_id) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'pending',?,?,?,(SELECT quota_reset_credit_id FROM virtual_accounts WHERE id=?))")
            .bind(&record.provider_id).bind(&record.subject_kind).bind(&record.id).bind(&record.account_id).bind(&record.account_name).bind(&record.subject_id).bind(&record.subject_name)
            .bind(&record.endpoint).bind(&record.transport).bind(&record.model).bind(&record.reasoning_effort).bind(&record.service_tier).bind(&record.image_size).bind(record.requested_at_ms).bind(&record.status)
            .bind(&record.actual_model).bind(snapshot).bind(&record.error_code).bind(&record.error_message).bind(&record.upstream_request_id).bind(&record.subject_id)
            .execute(self.pool()).await?;
        Ok(())
    }

    pub async fn finish_usage(&self, record: &UsageRecord) -> Result<()> {
        let snapshot: Option<Option<String>> =
            sqlx::query_scalar("SELECT pricing_snapshot_json FROM usage_records WHERE id=?")
                .bind(&record.id)
                .fetch_optional(self.pool())
                .await?;
        let prices = snapshot
            .flatten()
            .map(|s| serde_json::from_str::<crate::billing::BillingSnapshot>(&s))
            .transpose()?;
        let (cost, billing_status, billing_model) = match prices {
            Some(prices) => prices.charge(record),
            None => (None, "legacy", None),
        };
        sqlx::query("UPDATE usage_records SET cost_nano_usd=?,billing_status=?,billing_model=?,service_tier=?,actual_model=?, input_tokens=?,output_tokens=?,cached_tokens=?,cache_write_tokens=?,reasoning_tokens=?,image_size=?,first_byte_ms=?,total_ms=?,status=?,http_status=?,image_count=?,image_usage_json=?,error_code=?,error_message=?,upstream_request_id=? WHERE id=? AND status='in_progress'")
            .bind(cost).bind(billing_status).bind(billing_model).bind(&record.service_tier).bind(&record.actual_model).bind(record.input_tokens).bind(record.output_tokens).bind(record.cached_tokens).bind(record.cache_write_tokens)
            .bind(record.reasoning_tokens).bind(&record.image_size).bind(record.first_byte_ms).bind(record.total_ms).bind(&record.status).bind(record.http_status).bind(record.image_count).bind(&record.image_usage_json).bind(&record.error_code).bind(&record.error_message).bind(&record.upstream_request_id).bind(&record.id)
            .execute(self.pool()).await?;
        Ok(())
    }

    pub async fn query_usage(&self, filter: &UsageFilter) -> Result<UsagePage> {
        let page_size = table_page_size(filter.page_size)?;
        let mut tx = self.pool().begin().await?;
        let mut count = QueryBuilder::new("SELECT COUNT(*) FROM usage_records");
        conditions(&mut count, filter);
        let total: i64 = count.build_query_scalar().fetch_one(&mut *tx).await?;
        let max_page = ((total.saturating_sub(1) / page_size) + 1).max(1) as u32;
        let page = filter.page.max(1).min(max_page);
        let mut query = QueryBuilder::new("SELECT * FROM usage_records");
        conditions(&mut query, filter);
        query
            .push(" ORDER BY requested_at_ms DESC, id DESC LIMIT ")
            .push_bind(page_size)
            .push(" OFFSET ")
            .push_bind(i64::from(page - 1) * page_size);
        let records = query
            .build_query_as::<UsageRecord>()
            .fetch_all(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(UsagePage {
            records,
            total,
            page,
            page_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_profile_preserves_unknown_usage_and_counts_calendar_streaks() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 18).unwrap();
        let day = |offset: i64, tokens: Option<i64>, duration: Option<i64>| AccountDailyUsage {
            start_date: today + chrono::TimeDelta::days(offset),
            tokens,
            longest_running_turn_ms: duration,
        };
        let summary = account_usage_summary(
            vec![
                day(-5, Some(10), Some(1200)),
                day(-4, Some(20), Some(4100)),
                day(-2, None, None),
                day(-1, Some(0), Some(0)),
                day(0, Some(30), Some(2500)),
            ],
            today,
        );
        assert_eq!(summary.lifetime_tokens, Some(60));
        assert_eq!(summary.peak_daily_tokens, Some(30));
        assert_eq!(summary.current_streak_days, 3);
        assert_eq!(summary.longest_streak_days, 3);
        assert_eq!(summary.longest_running_turn_sec, Some(4));
        assert_eq!(summary.daily_usage_buckets.len(), 6);
        assert_eq!(summary.daily_usage_buckets[2].tokens, Some(0));
        assert_eq!(summary.daily_usage_buckets[3].tokens, None);
        let stale = account_usage_summary(vec![day(-3, None, None)], today);
        assert_eq!(stale.current_streak_days, 0);
        assert_eq!(stale.lifetime_tokens, None);
        let empty = account_usage_summary(vec![], today);
        assert_eq!(empty.lifetime_tokens, Some(0));
        assert_eq!(empty.longest_streak_days, 0);
    }
    #[tokio::test]
    async fn usage_history_persists_filters_paginates_and_keeps_snapshot_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.sqlite");
        let storage = Storage::open(&path).await.unwrap();
        for n in 0..53 {
            let record = UsageRecord {
                id: format!("row-{n:03}"),
                account_id: format!("account-{}", n % 2),
                account_name: if n % 2 == 0 {
                    "Alice %".into()
                } else {
                    "Bob".into()
                },
                subject_id: format!("key-{}", n % 2),
                subject_name: format!("name-{}", n % 2),
                endpoint: "/v1/responses".into(),
                transport: "http".into(),
                model: Some("model-test".into()),
                reasoning_effort: Some("xhigh".into()),
                service_tier: Some("priority".into()),
                requested_at_ms: 1000 + n,
                status: "in_progress".into(),
                ..Default::default()
            };
            storage.insert_usage(&record).await.unwrap();
        }
        let first = storage.query_usage(&UsageFilter::default()).await.unwrap();
        assert_eq!(first.total, 53);
        assert_eq!(first.records.len(), 20);
        assert_eq!(first.records[0].id, "row-052");
        let second = storage
            .query_usage(&UsageFilter {
                page: 2,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(second.records.len(), 20);
        assert_eq!(second.records[0].id, "row-032");
        for size in [10, 20, 30, 50] {
            let last = storage
                .query_usage(&UsageFilter {
                    page: 999,
                    page_size: Some(size),
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(last.page_size, i64::from(size));
            assert_eq!(last.page, 53_u32.div_ceil(size));
            assert_eq!(last.records.len(), (53 % size) as usize);
            assert_eq!(last.records.last().unwrap().id, "row-000");
        }
        let filtered = storage
            .query_usage(&UsageFilter {
                account: Some("alice %".into()),
                subject_id: Some("key-0".into()),
                model: Some("MODEL".into()),
                from_ms: Some(1010),
                until_ms: Some(1015),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(filtered.total, 3);
        assert_eq!(
            storage
                .query_usage(&UsageFilter {
                    account: Some("' OR 1=1 --".into()),
                    ..Default::default()
                })
                .await
                .unwrap()
                .total,
            0
        );
        let mut record = filtered.records[0].clone();
        record.status = "completed".into();
        record.input_tokens = Some(0);
        record.first_byte_ms = Some(120);
        record.total_ms = Some(900);
        storage.finish_usage(&record).await.unwrap();
        record.input_tokens = Some(99);
        storage.finish_usage(&record).await.unwrap(); // Finalization is idempotent.
        assert!(
            storage
                .search_supplier_accounts("", 5, None, false)
                .await
                .unwrap()
                .is_empty()
        );
        let completed = storage
            .query_usage(&UsageFilter {
                status: Some("completed".into()),
                account: Some("lic".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(completed.total, 1);
        assert_eq!(completed.records[0].id, record.id);
        storage.close().await;
        let storage = Storage::open(&path).await.unwrap();
        storage.recover_interrupted_usage().await.unwrap();
        let page = storage
            .query_usage(&UsageFilter {
                from_ms: Some(1014),
                until_ms: Some(1015),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.records[0].input_tokens, Some(0));
        assert_eq!(page.records[0].reasoning_effort.as_deref(), Some("xhigh"));
        assert_eq!(page.records[0].service_tier.as_deref(), Some("priority"));
        assert_eq!(page.records[0].account_name, "Alice %");
        assert_eq!(page.records[0].total_ms, Some(900));
        let interrupted = storage
            .query_usage(&UsageFilter {
                status: Some("interrupted".into()),
                page: 2,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(interrupted.total, 52);
        assert_eq!(interrupted.records.len(), 20);
        assert!(
            interrupted
                .records
                .iter()
                .all(|r| r.status == "interrupted")
        );
        assert_eq!(
            storage
                .query_usage(&UsageFilter::default())
                .await
                .unwrap()
                .records[0]
                .status,
            "interrupted"
        );
        storage.close().await;
    }

    #[tokio::test]
    async fn deleted_consumers_keep_inspectable_usage_history() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("history.sqlite"))
            .await
            .unwrap();
        let consumer = crate::VirtualAccount {
            provider_id: "chatgpt".into(),
            id: "consumer".into(),
            username: "consumer".into(),
            password_hash: "fixture".into(),
            name: "Consumer".into(),
            email: "consumer@example.test".into(),
            plan_type: "plus".into(),
            plan_id: "plus".into(),
            subscription_expires_at: None,
            enabled: true,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        storage.save_virtual_account(&consumer).await.unwrap();
        storage
            .insert_usage(&UsageRecord {
                id: "record".into(),
                subject_id: consumer.id.clone(),
                subject_name: consumer.name.clone(),
                account_id: "supplier".into(),
                account_name: "Supplier".into(),
                status: "completed".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            storage.search_virtual_accounts("", 5).await.unwrap()[0].id,
            consumer.id
        );
        storage.delete_virtual_account(&consumer.id).await.unwrap();
        assert!(
            storage
                .search_virtual_accounts("", 5)
                .await
                .unwrap()
                .is_empty()
        );
        let history = storage
            .query_usage(&UsageFilter {
                subject_id: Some(consumer.id.clone()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(history.total, 1);
        assert_eq!(history.records[0].subject_name, "Consumer");
    }
}

//! Shared SQL selection and atomic administrator operations for subscriptions.
use crate::{PlatformAccount, Result, Storage, StorageError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, Postgres, QueryBuilder};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SubscriptionFilters {
    pub user_id: String,
    pub provider_id: String,
    pub plan_id: String,
    pub search: String,
    pub status: String,
    pub subscription: String,
}

impl SubscriptionFilters {
    pub fn validate(&self) -> Result<()> {
        if self.user_id.len() > 128
            || self.provider_id.len() > 64
            || self.plan_id.len() > 128
            || self.search.len() > 1024
            || !matches!(self.status.as_str(), "" | "enabled" | "disabled")
            || !matches!(self.subscription.as_str(), "" | "active" | "expired")
        {
            return Err(StorageError::InvalidAdminUpdate("账户筛选条件无效"));
        }
        Ok(())
    }

    pub(crate) fn conditions(&self, query: &mut QueryBuilder<'_, Postgres>, now: i64) {
        // Both the visible list and 'all matching' use this exact predicate.
        query.push(" WHERE 1=1");
        for (column, value) in [
            ("user_id", &self.user_id),
            ("provider_id", &self.provider_id),
            ("plan_id", &self.plan_id),
        ] {
            if !value.is_empty() {
                query
                    .push(" AND ")
                    .push(column)
                    .push("=")
                    .push_bind(value.clone());
            }
        }
        if !self.search.trim().is_empty() {
            query
                .push(" AND strpos(lower(name || ' ' || username || ' ' || email),lower(")
                .push_bind(self.search.trim().to_owned())
                .push("))>0");
        }
        match self.status.as_str() {
            "enabled" => {
                query.push(" AND enabled=TRUE");
            }
            "disabled" => {
                query.push(" AND enabled=FALSE");
            }
            _ => {}
        }
        match self.subscription.as_str() {
            "active" => {
                query.push(" AND (subscription_expires_at IS NULL OR floor(extract(epoch FROM (subscription_expires_at)::timestamptz))::bigint>").push_bind(now).push(")");
            }
            "expired" => {
                query.push(" AND subscription_expires_at IS NOT NULL AND COALESCE(floor(extract(epoch FROM (subscription_expires_at)::timestamptz))::bigint,0)<=").push_bind(now);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResetCardGrant {
    pub quantity: i64,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub activate_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "default_duration")]
    pub duration_days: i64,
}
fn default_duration() -> i64 {
    30
}
impl ResetCardGrant {
    pub fn validate(&self) -> Result<()> {
        if !(1..=100).contains(&self.quantity) || self.note.chars().count() > 256 {
            return Err(StorageError::InvalidAdminUpdate(
                "每次发放 1 至 100 张，备注最多 256 字",
            ));
        }
        if !(1..=3650).contains(&self.duration_days) {
            return Err(StorageError::InvalidAdminUpdate(
                "有效时长须为 1 至 3650 天",
            ));
        }
        self.times(chrono::Utc::now().timestamp_millis())?;
        Ok(())
    }
    pub(crate) fn times(&self, now: i64) -> Result<(i64, i64)> {
        let start = self.activate_at.map_or(now, |date| date.timestamp_millis());
        let end = start
            .checked_add(
                self.duration_days
                    .checked_mul(86_400_000)
                    .ok_or(StorageError::InvalidAdminUpdate("有效时长无效"))?,
            )
            .filter(|end| {
                *end > start
                    && start >= 0
                    && chrono::DateTime::from_timestamp_millis(*end).is_some()
            })
            .ok_or(StorageError::InvalidAdminUpdate(
                "重置卡启用时间和有效时长无效",
            ))?;
        Ok((start, end))
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionSelection {
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default)]
    pub all_matching: bool,
    #[serde(default)]
    pub filters: SubscriptionFilters,
    #[serde(default)]
    pub excluded_ids: Vec<String>,
}
impl SubscriptionSelection {
    fn normalize(&mut self) -> Result<()> {
        self.filters.search = self.filters.search.trim().into();
        self.filters.validate()?;
        if self.ids.len() > 1000
            || self.excluded_ids.len() > 1000
            || self
                .ids
                .iter()
                .chain(&self.excluded_ids)
                .any(|id| id.trim().is_empty() || id.len() > 256)
            || (self.all_matching && !self.ids.is_empty())
            || (!self.all_matching && (self.ids.is_empty() || !self.excluded_ids.is_empty()))
        {
            return Err(StorageError::InvalidAdminUpdate(
                "请选择账户或筛选结果；手动选择及排除最多 1000 个账户",
            ));
        }
        self.ids.sort();
        self.ids.dedup();
        self.excluded_ids.sort();
        self.excluded_ids.dedup();
        Ok(())
    }
    fn query(&self, now: i64, columns: &str) -> QueryBuilder<'static, Postgres> {
        let mut q = QueryBuilder::new(format!("SELECT {columns} FROM regular_platforms"));
        if self.all_matching {
            self.filters.conditions(&mut q, now);
            if !self.excluded_ids.is_empty() {
                ids_clause(&mut q, " AND id NOT IN (", &self.excluded_ids);
            }
        } else {
            ids_clause(&mut q, " WHERE id IN (", &self.ids);
        }
        q
    }
}
fn ids_clause(q: &mut QueryBuilder<'_, Postgres>, prefix: &str, ids: &[String]) {
    q.push(prefix);
    let mut separated = q.separated(",");
    for id in ids {
        separated.push_bind(id.clone());
    }
    q.push(")");
}

pub(crate) async fn replay(
    connection: &mut PgConnection,
    scope: &str,
    id: &str,
    signature: &str,
) -> Result<Option<Value>> {
    crate::reset_credits::request_id(id)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended(current_schema() || ':operation:' || $1 || ':' || $2,0))")
        .bind(scope).bind(id).execute(&mut *connection).await?;
    let previous: Option<(String,String)> = sqlx::query_as("SELECT signature,result_json FROM admin_subscription_operations WHERE scope=$1 AND request_id=$2")
        .bind(scope).bind(id).fetch_optional(connection).await?;
    if let Some((stored, result)) = previous {
        if stored != signature {
            return Err(StorageError::InvalidAdminUpdate(
                "同一请求标识不能用于不同操作或参数",
            ));
        }
        return Ok(Some(serde_json::from_str(&result)?));
    }
    Ok(None)
}
pub(crate) async fn remember(
    connection: &mut PgConnection,
    scope: &str,
    id: &str,
    signature: &str,
    result: &Value,
    now: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO admin_subscription_operations VALUES($1,$2,$3,$4,$5)")
        .bind(scope)
        .bind(id)
        .bind(signature)
        .bind(result.to_string())
        .bind(now)
        .execute(connection)
        .await?;
    Ok(())
}

impl Storage {
    /// Current configured quota windows only; never query lifetime billing or a supplier.
    pub async fn virtual_list_quota(&self, account: &PlatformAccount) -> Result<Value> {
        let now = chrono::Utc::now().timestamp();
        let account = self
            .platform_account(&account.id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(account.id.clone()))?;
        let plan = self
            .virtual_plan(&account.plan_id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(account.plan_id.clone()))?;
        let rules = crate::plan_spending_windows(&plan.config)?;
        let anchor = self.subscription_anchor(&account.id).await?;
        let windows = self
            .nested_spending_windows(
                &account.id,
                &rules,
                if anchor > now { 0 } else { anchor },
                now,
            )
            .await?;
        Ok(
            json!({"windows":windows.iter().enumerate().map(|(index,w)|json!({
            "id":if index==0{"primary_window"}else{"secondary_window"},
            "used_percent":w["used_percent"],"limit_window_seconds":w["limit_window_seconds"],"reset_at":w["reset_at"]
        })).collect::<Vec<_>>()}),
        )
    }

    pub async fn platform_account_page(
        &self,
        filters: &SubscriptionFilters,
        page: u32,
        page_size: Option<u32>,
    ) -> Result<(Vec<PlatformAccount>, i64, u32)> {
        filters.validate()?;
        let size = crate::table_page_size(page_size)?;
        let now = chrono::Utc::now().timestamp();
        let mut tx = self
            .pool()
            .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .await?;
        let mut count = QueryBuilder::new("SELECT COUNT(*) FROM regular_platforms");
        filters.conditions(&mut count, now);
        let total: i64 = count.build_query_scalar().fetch_one(&mut *tx).await?;
        let page = page
            .max(1)
            .min(((total.saturating_sub(1) / size) + 1) as u32);
        let mut q = QueryBuilder::new("SELECT * FROM regular_platforms");
        filters.conditions(&mut q, now);
        q.push(" ORDER BY id LIMIT ")
            .push_bind(size)
            .push(" OFFSET ")
            .push_bind(i64::from(page - 1) * size);
        let rows = q.build_query_as().fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok((rows, total, page))
    }

    pub async fn subscription_batch(
        &self,
        request_id: &str,
        operation: &str,
        mut selection: SubscriptionSelection,
        grant: Option<ResetCardGrant>,
    ) -> Result<Value> {
        selection.normalize()?;
        if !matches!(operation, "delete" | "reset" | "grant_reset")
            || ((operation == "grant_reset") != grant.is_some())
        {
            return Err(StorageError::InvalidAdminUpdate(
                "批量操作类型或发卡参数无效",
            ));
        }
        if let Some(grant) = &grant {
            grant.validate()?;
        }
        let signature =
            json!({"operation":operation,"selection":selection,"grant":grant}).to_string();
        let mut tx = self.pool().begin().await?;
        if let Some(result) = replay(&mut tx, "batch", request_id, &signature).await? {
            tx.commit().await?;
            return Ok(result);
        }
        let now = chrono::Utc::now().timestamp_millis();
        let matched: i64 = selection
            .query(now / 1000, "COUNT(*)")
            .build_query_scalar()
            .fetch_one(&mut *tx)
            .await?;
        if !selection.all_matching && matched != selection.ids.len() as i64 {
            return Err(StorageError::InvalidAdminUpdate(
                "所选账户已被删除，请刷新后重试",
            ));
        }
        let mut cursor = String::new();
        let mut affected = 0_i64;
        loop {
            let mut q = selection.query(now / 1000, "id");
            q.push(" AND id>")
                .push_bind(cursor.clone())
                .push(" ORDER BY id LIMIT 200");
            let ids: Vec<String> = q.build_query_scalar().fetch_all(&mut *tx).await?;
            if ids.is_empty() {
                break;
            }
            cursor = ids.last().unwrap().clone();
            for id in ids {
                crate::storage::lock_subscription(&mut tx, &id).await?;
                match operation {
                    "delete" => {
                        crate::user_subscriptions::delete_on(&mut tx, &id).await?;
                        affected += 1;
                    }
                    "reset" => {
                        if crate::reset_credits::admin_reset_on(&mut tx, &id, "admin", now).await?["code"]
                            == "reset"
                        {
                            affected += 1;
                        }
                    }
                    "grant_reset" => {
                        let grant = grant.as_ref().unwrap();
                        let (start, end) = grant.times(now)?;
                        crate::reset_credits::grant_on(
                            &mut tx,
                            &id,
                            &uuid::Uuid::new_v4().to_string(),
                            grant.quantity,
                            &grant.note,
                            start,
                            Some(end),
                            now,
                        )
                        .await?;
                        affected += 1;
                    }
                    _ => unreachable!(),
                }
            }
        }
        let result =
            json!({"ok":true,"matched":matched,"affected":affected,"skipped":matched-affected});
        remember(&mut tx, "batch", request_id, &signature, &result, now).await?;
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::Subscriptions]);
        Ok(result)
    }
}

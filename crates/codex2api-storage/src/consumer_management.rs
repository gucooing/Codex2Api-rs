//! Shared SQL selection and atomic administrator operations for consumers.
use crate::{Result, Storage, StorageError, VirtualAccount};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConsumerFilters {
    pub search: String,
    pub status: String,
    pub subscription: String,
}

impl ConsumerFilters {
    pub fn validate(&self) -> Result<()> {
        if self.search.len() > 1024
            || !matches!(self.status.as_str(), "" | "enabled" | "disabled")
            || !matches!(
                self.subscription.as_str(),
                "" | "active" | "expired" | "free"
            )
        {
            return Err(StorageError::InvalidAdminUpdate("账户筛选条件无效"));
        }
        Ok(())
    }

    pub(crate) fn conditions(&self, query: &mut QueryBuilder<'_, Sqlite>, now: i64) {
        // Both the visible list and 'all matching' use this exact predicate.
        query.push(" WHERE 1=1");
        if !self.search.trim().is_empty() {
            query
                .push(" AND instr(lower(name || ' ' || username || ' ' || email),lower(")
                .push_bind(self.search.trim().to_owned())
                .push("))>0");
        }
        match self.status.as_str() {
            "enabled" => {
                query.push(" AND enabled=1");
            }
            "disabled" => {
                query.push(" AND enabled=0");
            }
            _ => {}
        }
        match self.subscription.as_str() {
            "free" => {
                query.push(" AND plan_type='free'");
            }
            "active" => {
                query.push(" AND plan_type!='free' AND (subscription_expires_at IS NULL OR unixepoch(subscription_expires_at)>").push_bind(now).push(")");
            }
            "expired" => {
                query.push(" AND plan_type!='free' AND subscription_expires_at IS NOT NULL AND COALESCE(unixepoch(subscription_expires_at),0)<=").push_bind(now);
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
pub struct ConsumerSelection {
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default)]
    pub all_matching: bool,
    #[serde(default)]
    pub filters: ConsumerFilters,
    #[serde(default)]
    pub excluded_ids: Vec<String>,
}
impl ConsumerSelection {
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
    fn query(&self, now: i64, columns: &str) -> QueryBuilder<'static, Sqlite> {
        let mut q = QueryBuilder::new(format!("SELECT {columns} FROM virtual_accounts"));
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
fn ids_clause(q: &mut QueryBuilder<'_, Sqlite>, prefix: &str, ids: &[String]) {
    q.push(prefix);
    let mut separated = q.separated(",");
    for id in ids {
        separated.push_bind(id.clone());
    }
    q.push(")");
}

pub(crate) async fn replay(
    connection: &mut SqliteConnection,
    scope: &str,
    id: &str,
    signature: &str,
) -> Result<Option<Value>> {
    crate::reset_credits::request_id(id)?;
    let previous: Option<(String,String)> = sqlx::query_as("SELECT signature,result_json FROM admin_consumer_operations WHERE scope=? AND request_id=?")
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
    connection: &mut SqliteConnection,
    scope: &str,
    id: &str,
    signature: &str,
    result: &Value,
    now: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO admin_consumer_operations VALUES(?,?,?,?,?)")
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
    pub async fn virtual_list_quota(&self, account: &VirtualAccount) -> Result<Value> {
        let now = chrono::Utc::now().timestamp();
        let plan = self
            .virtual_plan(&account.plan_id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(account.plan_id.clone()))?;
        let rules = crate::plan_spending_windows(
            &plan.config,
            account.effective_plan_at(now) == "free" && account.plan_type != "free",
        )?;
        let anchor:i64=sqlx::query_scalar("SELECT COALESCE(unixepoch(subscription_started_at),unixepoch(created_at)) FROM virtual_accounts WHERE id=?")
            .bind(&account.id).fetch_one(self.pool()).await?;
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

    pub async fn virtual_account_page(
        &self,
        filters: &ConsumerFilters,
        page: u32,
        page_size: Option<u32>,
    ) -> Result<(Vec<VirtualAccount>, i64, u32)> {
        filters.validate()?;
        let size = crate::table_page_size(page_size)?;
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool().begin().await?;
        let mut count = QueryBuilder::new("SELECT COUNT(*) FROM virtual_accounts");
        filters.conditions(&mut count, now);
        let total: i64 = count.build_query_scalar().fetch_one(&mut *tx).await?;
        let page = page
            .max(1)
            .min(((total.saturating_sub(1) / size) + 1) as u32);
        let mut q = QueryBuilder::new("SELECT * FROM virtual_accounts");
        filters.conditions(&mut q, now);
        q.push(" ORDER BY id LIMIT ")
            .push_bind(size)
            .push(" OFFSET ")
            .push_bind(i64::from(page - 1) * size);
        let rows = q.build_query_as().fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok((rows, total, page))
    }

    pub async fn consumer_batch(
        &self,
        request_id: &str,
        operation: &str,
        mut selection: ConsumerSelection,
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
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
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
                match operation {
                    "delete" => {
                        sqlx::query("DELETE FROM virtual_accounts WHERE id=?")
                            .bind(&id)
                            .execute(&mut *tx)
                            .await?;
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
        Ok(result)
    }
}

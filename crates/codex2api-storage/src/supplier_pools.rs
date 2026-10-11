use crate::{Result, Storage, StorageError};
use sqlx::FromRow;

#[derive(Clone, Debug, FromRow, serde::Serialize)]
pub struct SupplierTag {
    pub id: String,
    pub provider_id: String,
    pub name: String,
    pub supplier_count: i64,
    pub binding_count: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, FromRow)]
pub struct PoolCandidate {
    pub id: String,
    pub concurrency: u32,
    pub priority: i32,
    pub load_factor: Option<u32>,
}
impl Storage {
    pub async fn pool_contains_supplier(
        &self,
        owner: &str,
        provider: &str,
        supplier: Option<&str>,
    ) -> Result<bool> {
        let Some(supplier) = supplier else {
            return Ok(false);
        };
        let Some(tag) = self
            .execution_route(owner, provider)
            .await?
            .and_then(|r| r.tag_id)
        else {
            return Ok(false);
        };
        Ok(self
            .pool_candidates(&tag, provider, None)
            .await?
            .iter()
            .any(|a| a.id == supplier))
    }
    pub async fn pool_candidates(
        &self,
        tag: &str,
        provider: &str,
        model: Option<&str>,
    ) -> Result<Vec<PoolCandidate>> {
        let key = format!("pool:{}", serde_json::to_string(&(tag, provider, model))?);
        if let Some(value) = self.runtime.cached(&key) {
            return Ok(value);
        }
        let epoch = self.runtime.epoch();
        let rows = sqlx::query_as(
            "SELECT a.id,COALESCE(s.concurrency,5) AS concurrency,
                    COALESCE(s.priority,0) AS priority,s.load_factor
             FROM supplier_tag_members m
             JOIN supplier_accounts a ON a.id=m.account_id
             JOIN supplier_tokens k ON k.account_id=a.id
             LEFT JOIN supplier_health h ON h.account_id=a.id
             LEFT JOIN supplier_scheduling s ON s.account_id=a.id
             WHERE m.tag_id=? AND a.provider_id=? AND a.status='active'
               AND COALESCE(k.access_token,'')!=''
               AND (h.rejected_auth_revision IS NULL OR h.rejected_auth_revision!=a.auth_revision)
               AND h.payment_required_at IS NULL
               AND (COALESCE(h.cooldown_kind,'')!='quota_exhausted'
                    OR h.cooldown_auth_revision IS NULL OR h.cooldown_auth_revision!=a.auth_revision
                    OR h.cooldown_until<=?)
               AND (?='' OR EXISTS(SELECT 1 FROM supplier_models sm WHERE sm.account_id=a.id AND sm.model=?))
             ORDER BY a.id",
        )
        .bind(tag)
        .bind(provider)
        .bind(chrono::Utc::now().timestamp())
        .bind(model.unwrap_or(""))
        .bind(model.unwrap_or(""))
        .fetch_all(self.pool())
        .await?;
        self.runtime
            .cache(key, &rows, epoch, std::time::Duration::from_secs(2));
        Ok(rows)
    }
    pub async fn supplier_tags(&self) -> Result<Vec<SupplierTag>> {
        Ok(sqlx::query_as(
            "SELECT t.*,
            (SELECT COUNT(*) FROM supplier_tag_members m WHERE m.tag_id=t.id) AS supplier_count,
            (SELECT COUNT(*) FROM effective_execution_routes r WHERE r.tag_id=t.id) AS binding_count
            FROM supplier_tags t ORDER BY t.provider_id,t.name,t.id",
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn save_supplier_tag(&self, id: &str, provider: &str, name: &str) -> Result<()> {
        if name.trim().is_empty()
            || name.trim().chars().count() > 80
            || name.chars().any(char::is_control)
        {
            return Err(StorageError::Constraint(
                "标签名称须为 1 到 80 个字符".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO supplier_tags(id,provider_id,name) VALUES(?,?,?)
            ON CONFLICT(id) DO UPDATE SET name=excluded.name,provider_id=excluded.provider_id",
        )
        .bind(id)
        .bind(provider)
        .bind(name.trim())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_supplier_tag(&self, id: &str) -> Result<()> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let bound: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_routes WHERE tag_id=?) OR EXISTS(SELECT 1 FROM virtual_plans WHERE json_extract(config,'$.supplier_tag_id')=?)")
                .bind(id)
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if bound {
            return Err(StorageError::Constraint(
                "此标签仍用于套餐或虚拟账户，请先更换其号池".into(),
            ));
        }
        sqlx::query("DELETE FROM supplier_tags WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.invalidate_runtime_cache();
        Ok(())
    }

    pub async fn supplier_tag_ids(&self, account: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT tag_id FROM supplier_tag_members WHERE account_id=? ORDER BY tag_id",
        )
        .bind(account)
        .fetch_all(self.pool())
        .await?)
    }

    /// Replace the selected accounts' complete tag sets atomically. An empty
    /// submitted set clears tags; callers must not submit unloaded placeholders.
    pub async fn replace_supplier_tags(&self, accounts: &[String], tags: &[String]) -> Result<()> {
        let accounts: std::collections::BTreeSet<_> = accounts.iter().collect();
        let tags: std::collections::BTreeSet<_> = tags.iter().collect();
        if accounts.is_empty() || accounts.len() > 1000 || tags.len() > 100 {
            return Err(StorageError::Constraint(
                "请选择 1 到 1000 个账户，标签不能超过 100 个".into(),
            ));
        }
        let account_json = serde_json::to_string(&accounts)?;
        let tag_json = serde_json::to_string(&tags)?;
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let found: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM supplier_accounts WHERE id IN(SELECT value FROM json_each(?))",
        )
        .bind(&account_json)
        .fetch_one(&mut *tx)
        .await?;
        if found != accounts.len() as i64 {
            return Err(StorageError::Constraint(
                "所选供应账户已变更，请刷新后重试".into(),
            ));
        }
        let providers:Vec<String>=sqlx::query_scalar("SELECT DISTINCT provider_id FROM supplier_accounts WHERE id IN(SELECT value FROM json_each(?))")
            .bind(&account_json).fetch_all(&mut *tx).await?;
        for provider in providers {
            let valid:i64=sqlx::query_scalar("SELECT COUNT(*) FROM supplier_tags WHERE provider_id=? AND id IN(SELECT value FROM json_each(?))")
                .bind(provider).bind(&tag_json).fetch_one(&mut *tx).await?;
            if valid != tags.len() as i64 {
                return Err(StorageError::Constraint(
                    "只能选择与供应账户同平台的标签".into(),
                ));
            }
        }
        sqlx::query("DELETE FROM supplier_tag_members WHERE account_id IN(SELECT value FROM json_each(?)) AND tag_id NOT IN(SELECT value FROM json_each(?))")
            .bind(&account_json).bind(&tag_json).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO supplier_tag_members(tag_id,account_id) SELECT t.value,a.value FROM json_each(?) t CROSS JOIN json_each(?) a WHERE 1 ON CONFLICT(tag_id,account_id) DO NOTHING")
            .bind(&tag_json).bind(&account_json).execute(&mut *tx).await?;
        tx.commit().await?;
        self.invalidate_runtime_cache();
        Ok(())
    }

    /// Batch add/remove is atomic and never replaces unrelated tags.
    pub async fn edit_supplier_tags(
        &self,
        accounts: &[String],
        tags: &[String],
        remove: bool,
    ) -> Result<()> {
        if accounts.is_empty() || accounts.len() > 1000 || tags.is_empty() || tags.len() > 100 {
            return Err(StorageError::Constraint(
                "请选择 1 到 1000 个账户和 1 到 100 个标签".into(),
            ));
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        for account in accounts {
            for tag in tags {
                let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM supplier_accounts a JOIN supplier_tags t ON t.provider_id=a.provider_id WHERE a.id=? AND t.id=?)")
                    .bind(account).bind(tag).fetch_one(&mut *tx).await?;
                if !valid {
                    return Err(StorageError::Constraint(
                        "标签与供应账户必须属于同一提供商".into(),
                    ));
                }
                let query = if remove {
                    "DELETE FROM supplier_tag_members WHERE tag_id=? AND account_id=?"
                } else {
                    "INSERT INTO supplier_tag_members(tag_id,account_id) VALUES(?,?) ON CONFLICT DO NOTHING"
                };
                sqlx::query(query)
                    .bind(tag)
                    .bind(account)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        self.invalidate_runtime_cache();
        Ok(())
    }

    pub async fn save_pool_route(
        &self,
        owner: &str,
        provider: &str,
        tag: Option<&str>,
        expected: Option<i64>,
    ) -> Result<bool> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::VirtualAccount)
            .await?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT revision FROM execution_routes WHERE virtual_account_id=? AND provider_id=?",
        )
        .bind(owner)
        .bind(provider)
        .fetch_optional(&mut *tx)
        .await?;
        if current != expected {
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_routes(virtual_account_id,provider_id,tag_id,revision) VALUES(?,?,?,?) ON CONFLICT(virtual_account_id,provider_id) DO UPDATE SET tag_id=excluded.tag_id,revision=excluded.revision")
            .bind(owner).bind(provider).bind(tag).bind(expected.unwrap_or(0)+1).execute(&mut *tx).await?;
        tx.commit().await?;
        self.invalidate_runtime_cache();
        Ok(true)
    }
    /// Non-generation operations need a healthy pool member, without an assignment.
    pub async fn select_pool_supplier(
        &self,
        owner: &str,
        provider: &str,
        excluded: &[String],
    ) -> Result<Option<String>> {
        let Some(route) = self.execution_route(owner, provider).await? else {
            return Ok(None);
        };
        let Some(tag) = route.tag_id else {
            return Ok(None);
        };
        let mut candidates = self.pool_candidates(&tag, provider, None).await?;
        candidates.retain(|a| !excluded.contains(&a.id));
        candidates.sort_by_key(|a| {
            let metrics = self.supplier_metrics(&a.id);
            (a.priority, metrics.active, metrics.last_used)
        });
        Ok(candidates.into_iter().next().map(|a| a.id))
    }
}

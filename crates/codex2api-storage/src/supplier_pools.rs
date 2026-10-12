use crate::{Result, Storage, StorageError};
use sqlx::FromRow;
use std::{sync::Arc, time::Duration};

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
    #[serde(skip)]
    #[sqlx(skip)]
    pub(crate) runtime_key: String,
    #[serde(skip)]
    #[sqlx(skip)]
    pub(crate) runtime_shard: usize,
}
#[derive(FromRow)]
struct CandidateRow {
    #[sqlx(flatten)]
    candidate: PoolCandidate,
    ready_at: Option<i64>,
    model: Option<String>,
}

#[derive(Default)]
struct PoolIndex {
    all: Arc<Vec<Arc<PoolCandidate>>>,
    by_id: std::collections::HashMap<String, Arc<PoolCandidate>>,
    by_model: std::collections::HashMap<String, Arc<Vec<Arc<PoolCandidate>>>>,
    models: std::collections::HashMap<String, Arc<std::collections::HashSet<String>>>,
    empty: Arc<Vec<Arc<PoolCandidate>>>,
}

impl Storage {
    async fn pool_index(&self, tag: &str, provider: &str) -> Result<Arc<PoolIndex>> {
        let key = format!("pool-index:{}", serde_json::to_string(&(provider, tag))?);
        if let Some(value) = self.runtime.cached(&key) {
            return Ok(value);
        }
        let _fill = self.runtime.cache_fill(&key).await;
        if let Some(value) = self.runtime.cached(&key) {
            return Ok(value);
        }
        let epoch = self.runtime.epoch();
        let rows = sqlx::query_as::<_, CandidateRow>(
            "SELECT a.id,COALESCE(s.concurrency,5) AS concurrency,
             COALESCE(s.priority,0) AS priority,s.load_factor,sm.model,
             CASE WHEN h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision
                  THEN h.cooldown_until END AS ready_at
             FROM supplier_tag_members m
             JOIN supplier_accounts a ON a.id=m.account_id
             JOIN supplier_tokens k ON k.account_id=a.id
             LEFT JOIN supplier_health h ON h.account_id=a.id
             LEFT JOIN supplier_scheduling s ON s.account_id=a.id
             LEFT JOIN supplier_models sm ON sm.account_id=a.id
             WHERE m.tag_id=? AND a.provider_id=? AND a.status='active'
             AND COALESCE(k.access_token,'')!=''
             AND (h.rejected_auth_revision IS NULL OR h.rejected_auth_revision!=a.auth_revision)
             AND h.payment_required_at IS NULL
             AND (COALESCE(h.cooldown_kind,'')!='quota_exhausted'
                  OR h.cooldown_auth_revision IS NULL OR h.cooldown_auth_revision!=a.auth_revision
                  OR h.cooldown_until IS NOT NULL)
             ORDER BY a.id,sm.model"
        ).bind(tag).bind(provider).fetch_all(self.pool()).await?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut ttl_ms = 30_000;
        let mut index = PoolIndex::default();
        let mut all = Vec::new();
        let mut models = std::collections::HashMap::<String, Vec<Arc<PoolCandidate>>>::new();
        for row in rows {
            if let Some(at) = row.ready_at.filter(|at| at.saturating_mul(1000) > now) {
                ttl_ms = ttl_ms.min(at.saturating_mul(1000).saturating_sub(now));
                continue;
            }
            let id = row.candidate.id.clone();
            let candidate = index.by_id.entry(id.clone()).or_insert_with(|| {
                let mut candidate = row.candidate;
                candidate.runtime_key = format!("supplier:{}", candidate.id);
                candidate.runtime_shard = self.runtime.shard_index(&candidate.runtime_key);
                let candidate = Arc::new(candidate);
                all.push(candidate.clone());
                candidate
            });
            if let Some(model) = row.model {
                models
                    .entry(model.clone())
                    .or_default()
                    .push(candidate.clone());
                Arc::make_mut(index.models.entry(id).or_default()).insert(model);
            }
        }
        index.all = Arc::new(all);
        index.by_model = models
            .into_iter()
            .map(|(model, values)| (model, Arc::new(values)))
            .collect();
        for (id, models) in &index.models {
            self.runtime.cache(
                format!("supplier-models:{id}"),
                models,
                epoch,
                Duration::from_millis(ttl_ms as u64),
            );
        }
        let index = Arc::new(index);
        self.runtime
            .cache(key, &index, epoch, Duration::from_millis(ttl_ms as u64));
        Ok(index)
    }

    /// Direct model-to-account expansion; no per-request account/model filtering.
    pub async fn pool_candidates(
        &self,
        tag: &str,
        provider: &str,
        model: Option<&str>,
    ) -> Result<Arc<Vec<Arc<PoolCandidate>>>> {
        let index = self.pool_index(tag, provider).await?;
        Ok(match model.filter(|model| !model.is_empty()) {
            Some(model) => index.by_model.get(model).unwrap_or(&index.empty).clone(),
            None => index.all.clone(),
        })
    }

    pub async fn pool_candidate(
        &self,
        tag: &str,
        provider: &str,
        model: Option<&str>,
        id: &str,
    ) -> Result<Option<Arc<PoolCandidate>>> {
        let index = self.pool_index(tag, provider).await?;
        if let Some(model) = model.filter(|model| !model.is_empty())
            && !index
                .models
                .get(id)
                .is_some_and(|models| models.contains(model))
        {
            return Ok(None);
        }
        Ok(index.by_id.get(id).cloned())
    }

    pub async fn pool_contains_supplier(
        &self,
        owner: &str,
        provider: &str,
        supplier: Option<&str>,
    ) -> Result<bool> {
        let Some(id) = supplier else {
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
            .pool_candidate(&tag, provider, None, id)
            .await?
            .is_some())
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
        let candidates = self.pool_candidates(&tag, provider, None).await?;
        let selected = candidates
            .iter()
            .filter(|a| !excluded.contains(&a.id))
            .min_by_key(|a| {
                let metrics = self.supplier_metrics(&a.id);
                (a.priority, metrics.active, metrics.last_used)
            });
        Ok(selected.map(|a| a.id.clone()))
    }
}

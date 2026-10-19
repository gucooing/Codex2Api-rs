use crate::{Result, Storage, StorageError, SupplierPool};
use sqlx::FromRow;
use std::{
    sync::{Arc, Weak},
    time::Duration,
};

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
    #[sqlx(try_from = "i64")]
    pub concurrency: u32,
    pub priority: i32,
    #[sqlx(try_from = "crate::pg_types::OptionalU32")]
    pub load_factor: Option<u32>,
    #[serde(skip)]
    #[sqlx(skip)]
    pub(crate) runtime_key: String,
    #[serde(skip)]
    #[sqlx(skip)]
    pub(crate) runtime_shard: usize,
}
#[derive(Clone, FromRow, serde::Serialize, serde::Deserialize)]
struct CandidateRow {
    #[sqlx(flatten)]
    candidate: PoolCandidate,
    ready_at: Option<i64>,
    model: Option<String>,
}

#[derive(Default)]
struct PoolIndex {
    all: Arc<SupplierPool>,
    by_id: std::collections::HashMap<String, Arc<PoolCandidate>>,
    by_model: std::collections::HashMap<String, Arc<SupplierPool>>,
    models: std::collections::HashMap<String, Arc<std::collections::HashSet<String>>>,
    empty: Arc<SupplierPool>,
}

#[derive(Clone)]
struct PoolUnion {
    candidates: Arc<SupplierPool>,
    sources: Vec<Weak<SupplierPool>>,
}
impl PoolUnion {
    fn matches(&self, sources: &[Arc<SupplierPool>]) -> bool {
        self.sources.len() == sources.len()
            && self.sources.iter().zip(sources).all(|(old, current)| {
                // A weak reference preserves allocation identity without owning
                // the obsolete source pool or its scheduling index.
                old.as_ptr() == Arc::as_ptr(current)
            })
    }
}

impl Storage {
    /// API group tags expand into the same scheduler candidates as subscription pools.
    /// One stable cache slot per provider, tag set and model; source changes replace it.
    pub async fn pool_candidates_for_tags(
        &self,
        tags: &[String],
        provider: &str,
        model: Option<&str>,
    ) -> Result<Arc<SupplierPool>> {
        let model = model.filter(|model| !model.is_empty());
        let tags = tags.iter().collect::<std::collections::BTreeSet<_>>();
        if tags.len() == 1 {
            return self
                .pool_candidates(tags.first().expect("one tag"), provider, model)
                .await;
        }
        let mut sources = Vec::with_capacity(tags.len());
        for tag in &tags {
            sources.push(self.pool_candidates(tag, provider, model).await?);
        }
        let key = format!(
            "pool-union:{}",
            serde_json::to_string(&(provider, tags, model))?
        );
        let epoch = self.runtime.snapshots.version(&key);
        let cached = || {
            self.runtime
                .snapshots
                .get::<PoolUnion>(&key)
                .filter(|value| value.matches(&sources))
                .map(|value| value.candidates)
        };
        if let Some(value) = cached() {
            return Ok(value);
        }
        let _fill = self.runtime.snapshots.fill(&key).await;
        if let Some(value) = cached() {
            return Ok(value);
        }
        let mut seen = std::collections::BTreeSet::new();
        let candidates = Arc::new(SupplierPool::new(
            sources
                .iter()
                .flat_map(|source| source.iter())
                .filter(|candidate| seen.insert(candidate.id.clone()))
                .cloned()
                .collect::<Vec<_>>(),
        ));
        self.runtime.snapshots.put(
            key,
            &PoolUnion {
                candidates: candidates.clone(),
                sources: sources.iter().map(Arc::downgrade).collect(),
            },
            epoch,
            Duration::from_secs(30),
        );
        Ok(candidates)
    }

    pub async fn pool_candidate_for_tags(
        &self,
        tags: &[String],
        provider: &str,
        model: Option<&str>,
        id: &str,
    ) -> Result<Option<Arc<PoolCandidate>>> {
        for tag in tags {
            if let Some(candidate) = self.pool_candidate(tag, provider, model, id).await? {
                return Ok(Some(candidate));
            }
        }
        Ok(None)
    }

    async fn pool_index(&self, tag: &str, provider: &str) -> Result<Arc<PoolIndex>> {
        let key = format!("pool-index:{}", serde_json::to_string(&(provider, tag))?);
        if let Some(value) = self.runtime.snapshots.get(&key) {
            return Ok(value);
        }
        let _fill = self.runtime.snapshots.fill(&key).await;
        if let Some(value) = self.runtime.snapshots.get(&key) {
            return Ok(value);
        }
        let epoch = self.runtime.snapshots.version(&key);
        let rows = self.runtime.snapshots.read(format!("pool-rows:{provider}:{tag}"), async {
        let rows = sqlx::query_as::<_, CandidateRow>(
            "SELECT a.id,COALESCE(s.concurrency,5) AS concurrency,
             COALESCE(s.priority,0) AS priority,s.load_factor,sm.model,
             GREATEST(CASE WHEN h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision
                  THEN h.cooldown_until END,
                  CASE WHEN c.cooldown_auth_revision=a.auth_revision THEN c.cooldown_until END) AS ready_at
             FROM supplier_tag_members m
             JOIN supplier_accounts a ON a.id=m.account_id
             JOIN supplier_tokens k ON k.account_id=a.id
             LEFT JOIN supplier_health h ON h.account_id=a.id
             LEFT JOIN supplier_scheduling s ON s.account_id=a.id
             LEFT JOIN supplier_models sm ON sm.account_id=a.id
             LEFT JOIN supplier_model_cooldowns c ON c.account_id=a.id AND c.model=sm.model
             WHERE m.tag_id=$1 AND a.provider_id=$2 AND a.status='active'
             AND COALESCE(k.access_token,'')!=''
             AND (h.rejected_auth_revision IS NULL OR h.rejected_auth_revision!=a.auth_revision)
             AND h.payment_required_at IS NULL
             AND (COALESCE(h.cooldown_kind,'')!='quota_exhausted'
                  OR h.cooldown_auth_revision IS NULL OR h.cooldown_auth_revision!=a.auth_revision
                  OR h.cooldown_until IS NOT NULL)
             ORDER BY a.id,sm.model"
        ).bind(tag).bind(provider).fetch_all(self.pool()).await?;
        Ok((rows, Duration::from_secs(30)))
        }).await?;
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
        index.all = Arc::new(SupplierPool::new(all));
        index.by_model = models
            .into_iter()
            .map(|(model, values)| (model, Arc::new(SupplierPool::new(values))))
            .collect();
        let index = Arc::new(index);
        self.runtime
            .snapshots
            .put(key, &index, epoch, Duration::from_millis(ttl_ms as u64));
        Ok(index)
    }

    /// Direct model-to-account expansion; no per-request account/model filtering.
    pub async fn pool_candidates(
        &self,
        tag: &str,
        provider: &str,
        model: Option<&str>,
    ) -> Result<Arc<SupplierPool>> {
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
            ((SELECT COUNT(*) FROM effective_execution_routes r WHERE r.tag_id=t.id)+(SELECT COUNT(*) FROM api_group_tags g WHERE g.tag_id=t.id)) AS binding_count
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
            "INSERT INTO supplier_tags(id,provider_id,name) VALUES($1,$2,$3)
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
        let mut tx = self.pool().begin().await?;
        let bound: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_routes WHERE tag_id=$1) OR EXISTS(SELECT 1 FROM virtual_plans WHERE (config::jsonb #>> '{supplier_tag_id}')=$2) OR EXISTS(SELECT 1 FROM api_group_tags WHERE tag_id=$3)")
                .bind(id)
                .bind(id)
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if bound {
            return Err(StorageError::Constraint(
                "此标签仍用于套餐、订阅或 API 组，请先更换关联标签".into(),
            ));
        }
        sqlx::query("DELETE FROM supplier_tags WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.runtime.snapshots.invalidate(&[
            crate::snapshots::Domain::Suppliers,
            crate::snapshots::Domain::Pools,
            crate::snapshots::Domain::Subscriptions,
        ]);
        Ok(())
    }

    pub async fn supplier_tag_ids(&self, account: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT tag_id FROM supplier_tag_members WHERE account_id=$1 ORDER BY tag_id",
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
        let account_ids: Vec<&str> = accounts.iter().map(|s| s.as_str()).collect();
        let tag_ids: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "SELECT id FROM supplier_accounts WHERE id=ANY($1) ORDER BY id FOR NO KEY UPDATE",
        )
        .bind(&account_ids)
        .execute(&mut *tx)
        .await?;
        let found: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM supplier_accounts WHERE id = ANY($1)")
                .bind(&account_ids)
                .fetch_one(&mut *tx)
                .await?;
        if found != accounts.len() as i64 {
            return Err(StorageError::Constraint(
                "所选供应账户已变更，请刷新后重试".into(),
            ));
        }
        let providers: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT provider_id FROM supplier_accounts WHERE id = ANY($1)",
        )
        .bind(&account_ids)
        .fetch_all(&mut *tx)
        .await?;
        for provider in providers {
            let valid: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM supplier_tags WHERE provider_id=$1 AND id = ANY($2)",
            )
            .bind(provider)
            .bind(&tag_ids)
            .fetch_one(&mut *tx)
            .await?;
            if valid != tags.len() as i64 {
                return Err(StorageError::Constraint(
                    "只能选择与供应账户同平台的标签".into(),
                ));
            }
        }
        sqlx::query("DELETE FROM supplier_tag_members WHERE account_id = ANY($1) AND NOT (tag_id = ANY($2))")
            .bind(&account_ids).bind(&tag_ids).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO supplier_tag_members(tag_id,account_id) SELECT t,a FROM unnest($1::text[]) t CROSS JOIN unnest($2::text[]) a WHERE TRUE ON CONFLICT(tag_id,account_id) DO NOTHING")
            .bind(&tag_ids).bind(&account_ids).execute(&mut *tx).await?;
        tx.commit().await?;
        self.runtime.snapshots.invalidate(&[
            crate::snapshots::Domain::Suppliers,
            crate::snapshots::Domain::Pools,
            crate::snapshots::Domain::Subscriptions,
        ]);
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
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "SELECT id FROM supplier_accounts WHERE id=ANY($1) ORDER BY id FOR NO KEY UPDATE",
        )
        .bind(accounts)
        .execute(&mut *tx)
        .await?;
        let valid: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM unnest($1::text[]) account CROSS JOIN unnest($2::text[]) tag WHERE NOT EXISTS(SELECT 1 FROM supplier_accounts a JOIN supplier_tags t ON t.provider_id=a.provider_id WHERE a.id=account AND t.id=tag))")
            .bind(accounts).bind(tags).fetch_one(&mut *tx).await?;
        if !valid {
            return Err(StorageError::Constraint(
                "标签与供应账户必须属于同一提供商".into(),
            ));
        }
        if remove {
            sqlx::query(
                "DELETE FROM supplier_tag_members WHERE account_id=ANY($1) AND tag_id=ANY($2)",
            )
            .bind(accounts)
            .bind(tags)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query("INSERT INTO supplier_tag_members(account_id,tag_id) SELECT account,tag FROM unnest($1::text[]) account CROSS JOIN unnest($2::text[]) tag ON CONFLICT DO NOTHING")
                .bind(accounts).bind(tags).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        self.runtime.snapshots.invalidate(&[
            crate::snapshots::Domain::Suppliers,
            crate::snapshots::Domain::Pools,
            crate::snapshots::Domain::Subscriptions,
        ]);
        Ok(())
    }

    pub async fn save_pool_route(
        &self,
        owner: &str,
        provider: &str,
        tag: Option<&str>,
        expected: Option<i64>,
    ) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        crate::account_scope::require_on(&mut tx, owner, crate::AccountScope::UserPlatform).await?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT revision FROM execution_routes WHERE virtual_account_id=$1 AND provider_id=$2",
        )
        .bind(owner)
        .bind(provider)
        .fetch_optional(&mut *tx)
        .await?;
        if current != expected {
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_routes(virtual_account_id,provider_id,tag_id,revision) VALUES($1,$2,$3,$4) ON CONFLICT(virtual_account_id,provider_id) DO UPDATE SET tag_id=excluded.tag_id,revision=excluded.revision")
            .bind(owner).bind(provider).bind(tag).bind(expected.unwrap_or(0)+1).execute(&mut *tx).await?;
        tx.commit().await?;
        self.runtime.snapshots.invalidate(&[
            crate::snapshots::Domain::Suppliers,
            crate::snapshots::Domain::Pools,
            crate::snapshots::Domain::Subscriptions,
        ]);
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

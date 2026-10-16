use crate::{ListPage, ListQuery, Result, Storage, StorageError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct SupplierScheduling {
    #[sqlx(try_from = "i64")]
    pub concurrency: u32,
    pub priority: i32,
    #[sqlx(try_from = "crate::pg_types::OptionalU32")]
    pub load_factor: Option<u32>,
    pub revision: i64,
}
impl Default for SupplierScheduling {
    fn default() -> Self {
        Self {
            concurrency: 5,
            priority: 0,
            load_factor: None,
            revision: 0,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct SupplierModel {
    pub model: String,
    pub source: String,
    pub observed_at: Option<String>,
}

impl Storage {
    pub async fn supplier_scheduling(&self, id: &str) -> Result<SupplierScheduling> {
        Ok(sqlx::query_as(
            "SELECT concurrency,priority,load_factor,revision
             FROM supplier_scheduling WHERE account_id=$1",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?
        .unwrap_or_default())
    }
    pub async fn save_supplier_scheduling(
        &self,
        id: &str,
        value: &SupplierScheduling,
    ) -> Result<bool> {
        if value.concurrency > 1_000_000
            || !(0..=1_000_000).contains(&value.priority)
            || value.load_factor.is_some_and(|n| n == 0 || n > 1_000_000)
        {
            return Err(StorageError::Constraint(
                "并发、优先级须为 0 到 1000000，负载容量须为 1 到 1000000".into(),
            ));
        }
        let query = if value.revision == 0 {
            "INSERT INTO supplier_scheduling(account_id,concurrency,priority,load_factor,revision)
             SELECT id,$1,$2,$3,1 FROM supplier_accounts WHERE id=$4 AND $5=0
             ON CONFLICT(account_id) DO NOTHING"
        } else {
            "UPDATE supplier_scheduling
             SET concurrency=$1,priority=$2,load_factor=$3,revision=revision+1
             WHERE account_id=$4 AND revision=$5"
        };
        let changed = sqlx::query(query)
            .bind(i64::from(value.concurrency))
            .bind(i64::from(value.priority))
            .bind(value.load_factor.map(i64::from))
            .bind(id)
            .bind(value.revision)
            .execute(self.pool())
            .await?
            .rows_affected();
        self.runtime.snapshots.invalidate(&[
            crate::snapshots::Domain::Suppliers,
            crate::snapshots::Domain::Pools,
        ]);
        Ok(changed == 1)
    }
    pub async fn supplier_models(&self, id: &str) -> Result<Vec<SupplierModel>> {
        Ok(sqlx::query_as(
            "SELECT model,source,observed_at FROM supplier_models WHERE account_id=$1 ORDER BY model",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await?)
    }
    pub async fn supplier_model_page(
        &self,
        id: &str,
        query: &ListQuery,
    ) -> Result<ListPage<SupplierModel>> {
        self.read_list(
            query,
            "model,source,observed_at",
            "supplier_models",
            "model",
            |sql| {
                sql.push(" AND account_id=").push_bind(id.to_owned());
                crate::list_query::search(sql, &["model"], &query.search);
            },
        )
        .await
    }
    pub async fn supplier_model_options(&self, id: &str, search: &str) -> Result<Vec<Value>> {
        let rows: Vec<(String, bool)> = sqlx::query_as(
            "SELECT model,bool_or(enabled) FROM (
             SELECT m.model,m.enabled FROM model_catalog m
             JOIN supplier_accounts a ON a.provider_id=m.provider_id
             WHERE a.id=$1 AND m.deleted=FALSE AND strpos(lower(m.model),lower($2))>0
             UNION ALL SELECT model,FALSE FROM supplier_models
             WHERE account_id=$3 AND strpos(lower(model),lower($4))>0)
             GROUP BY model ORDER BY model LIMIT 100",
        )
        .bind(id)
        .bind(search.trim())
        .bind(id)
        .bind(search.trim())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(model, enabled)| json!({"model":model,"enabled":enabled}))
            .collect())
    }
    pub async fn add_supplier_model(&self, id: &str, model: &str) -> Result<()> {
        if !codex2api_common::valid_model(model) {
            return Err(StorageError::Constraint("模型名称无效".into()));
        }
        let changed = sqlx::query(
            "INSERT INTO supplier_models(account_id,model,source)
             SELECT id,$1,'custom' FROM supplier_accounts WHERE id=$2
             ON CONFLICT(account_id,model) DO UPDATE SET source='custom'",
        )
        .bind(model)
        .bind(id)
        .execute(self.pool())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(StorageError::AccountNotFound(id.into()));
        }
        self.runtime.snapshots.invalidate(&[
            crate::snapshots::Domain::Suppliers,
            crate::snapshots::Domain::Pools,
        ]);
        Ok(())
    }
    /// Replace the complete availability list atomically for one provider's suppliers.
    pub async fn replace_supplier_models(&self, ids: &[String], models: &[String]) -> Result<()> {
        if ids.is_empty()
            || ids.len() > 10_000
            || models.len() > 2048
            || models
                .iter()
                .any(|model| !codex2api_common::valid_model(model))
        {
            return Err(StorageError::Constraint("账户或模型列表无效".into()));
        }
        let mut tx = self.pool().begin().await?;
        let mut provider = None;
        for id in ids {
            let current: String =
                sqlx::query_scalar("SELECT provider_id FROM supplier_accounts WHERE id=$1")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or_else(|| StorageError::AccountNotFound(id.clone()))?;
            if provider.as_ref().is_some_and(|value| value != &current) {
                return Err(StorageError::Constraint("请选择同一平台的供应账户".into()));
            }
            provider = Some(current);
        }
        for id in ids.iter().collect::<std::collections::BTreeSet<_>>() {
            sqlx::query("DELETE FROM supplier_models WHERE account_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            for model in models.iter().collect::<std::collections::BTreeSet<_>>() {
                sqlx::query(
                    "INSERT INTO supplier_models(account_id,model,source) VALUES($1,$2,'custom')",
                )
                .bind(id)
                .bind(model)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        self.runtime.snapshots.invalidate(&[
            crate::snapshots::Domain::Suppliers,
            crate::snapshots::Domain::Pools,
        ]);
        Ok(())
    }
    pub async fn remove_supplier_model(&self, id: &str, model: &str) -> Result<()> {
        sqlx::query("DELETE FROM supplier_models WHERE account_id=$1 AND model=$2")
            .bind(id)
            .bind(model)
            .execute(self.pool())
            .await?;
        self.runtime.snapshots.invalidate(&[
            crate::snapshots::Domain::Suppliers,
            crate::snapshots::Domain::Pools,
        ]);
        Ok(())
    }
    /// Discovery changes only supplier availability. Never writes catalog/pricing.
    pub async fn sync_supplier_models(
        &self,
        id: &str,
        auth_revision: i64,
        models: &[Value],
    ) -> Result<bool> {
        if models.len() > 2048 {
            return Err(StorageError::Constraint("上游模型列表过大".into()));
        }
        let mut tx = self.pool().begin().await?;
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM supplier_accounts WHERE id=$1 AND auth_revision=$2)",
        )
        .bind(id)
        .bind(auth_revision)
        .fetch_one(&mut *tx)
        .await?;
        if !valid {
            return Ok(false);
        }
        sqlx::query("DELETE FROM supplier_models WHERE account_id=$1 AND source='upstream'")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let now = chrono::Utc::now().to_rfc3339();
        for value in models {
            let model = value["model"]
                .as_str()
                .filter(|v| codex2api_common::valid_model(v))
                .ok_or_else(|| StorageError::Constraint("上游模型名称无效".into()))?;
            sqlx::query(
                "INSERT INTO supplier_models(account_id,model,source,descriptor_json,observed_at)
                 VALUES($1,$2,'upstream',$3,$4) ON CONFLICT(account_id,model)
                 DO UPDATE SET descriptor_json=excluded.descriptor_json,observed_at=excluded.observed_at",
            )
            .bind(id)
            .bind(model)
            .bind(value.to_string())
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO supplier_model_sync(account_id,observed_at,auth_revision) VALUES($1,$2,$3)
             ON CONFLICT(account_id) DO UPDATE
             SET observed_at=excluded.observed_at,auth_revision=excluded.auth_revision",
        )
        .bind(id)
        .bind(&now)
        .bind(auth_revision)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.runtime.snapshots.invalidate(&[
            crate::snapshots::Domain::Suppliers,
            crate::snapshots::Domain::Pools,
        ]);
        Ok(true)
    }
    pub async fn supplier_supports_model(&self, id: &str, model: &str) -> Result<bool> {
        Ok(self.supplier_model_snapshot(id).await?.contains(model))
    }
    pub async fn supplier_model_snapshot(
        &self,
        id: &str,
    ) -> Result<std::sync::Arc<std::collections::HashSet<String>>> {
        let key = format!("supplier-models:{id}");
        self.runtime
            .snapshots
            .read(key.clone(), async {
                let models: Vec<String> =
                    sqlx::query_scalar("SELECT model FROM supplier_models WHERE account_id=$1")
                        .bind(id)
                        .fetch_all(self.pool())
                        .await?;
                let value = std::sync::Arc::new(
                    models.into_iter().collect::<std::collections::HashSet<_>>(),
                );
                Ok((value, std::time::Duration::from_secs(30)))
            })
            .await
    }
}

use crate::{Result, Storage, StorageError};
use serde_json::{Value, json};
impl Storage {
    /// Commit a complete authenticated catalog snapshot; failures preserve the last success.
    pub async fn save_grok_catalog(
        &self,
        id: &str,
        revision: i64,
        raw: &Value,
        models: &[Value],
    ) -> Result<bool> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM supplier_accounts WHERE id=? AND provider_id='grok' AND auth_revision=?)")
            .bind(id).bind(revision).fetch_one(&mut *tx).await?;
        if !valid {
            return Ok(false);
        }
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("DELETE FROM grok_model_observations WHERE account_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for value in models {
            let model = value["model"]
                .as_str()
                .filter(|s| codex2api_core::valid_model(s))
                .ok_or(StorageError::InvalidAdminUpdate("Grok 模型编号无效"))?;
            sqlx::query("INSERT INTO grok_model_observations(account_id,model,descriptor_json,observed_at,auth_revision) VALUES(?,?,?,?,?)")
                .bind(id).bind(model).bind(value.to_string()).bind(&now).bind(revision).execute(&mut *tx).await?;
            let changed=sqlx::query("INSERT INTO model_catalog(provider_id,model,kind,enabled) VALUES('grok',?,'text',0) ON CONFLICT(provider_id,model) DO NOTHING")
                .bind(model).execute(&mut *tx).await?.rows_affected();
            if changed > 0 {
                sqlx::query("INSERT OR IGNORE INTO meta(key,value) VALUES(?,'1')")
                    .bind(format!("model_preset_pending:grok:{model}"))
                    .execute(&mut *tx)
                    .await?;
            }
        }
        sqlx::query("INSERT INTO grok_catalog_snapshots(account_id,response_json,observed_at,auth_revision) VALUES(?,?,?,?) ON CONFLICT(account_id) DO UPDATE SET response_json=excluded.response_json,observed_at=excluded.observed_at,auth_revision=excluded.auth_revision")
            .bind(id).bind(raw.to_string()).bind(&now).bind(revision).execute(&mut *tx).await?;
        tx.commit().await?;
        let supported = models
            .iter()
            .filter_map(|v| v["model"].as_str())
            .map(|model| codex2api_core::SupportedModel {
                provider_id: codex2api_core::GROK.into(),
                model: model.into(),
                kind: "text".into(),
            })
            .collect::<Vec<_>>();
        self.sync_supported_models(&supported).await?;
        Ok(true)
    }
    pub async fn grok_model_descriptors(&self) -> Result<Vec<Value>> {
        let rows:Vec<(String,String,String)>=sqlx::query_as("SELECT model,descriptor_json,observed_at FROM grok_model_observations ORDER BY observed_at DESC,account_id").fetch_all(self.pool()).await?;
        let mut models = std::collections::BTreeMap::new();
        for (model, descriptor, _) in rows {
            models
                .entry(model)
                .or_insert(serde_json::from_str::<Value>(&descriptor)?);
        }
        Ok(models.into_values().collect())
    }
    pub async fn grok_catalog(&self, id: &str) -> Result<Value> {
        let snapshot: Option<(String, i64)> = sqlx::query_as(
            "SELECT observed_at,auth_revision FROM grok_catalog_snapshots WHERE account_id=?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        let rows:Vec<(String,String)>=sqlx::query_as("SELECT descriptor_json,observed_at FROM grok_model_observations WHERE account_id=? ORDER BY model").bind(id).fetch_all(self.pool()).await?;
        let items = rows
            .into_iter()
            .map(|(v, _)| serde_json::from_str::<Value>(&v))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let current = self.supplier_auth_revision(id).await?;
        Ok(
            json!({"items":items,"observed_at":snapshot.as_ref().map(|s|&s.0),"stale":snapshot.as_ref().is_none_or(|(_,r)|Some(*r)!=current)}),
        )
    }
    pub async fn grok_consumer_models(&self, owner: &str) -> Result<Vec<Value>> {
        let allowed = self
            .available_virtual_models(owner, codex2api_core::GROK)
            .await?;
        let descriptors = self.grok_model_descriptors().await?;
        Ok(allowed.into_iter().filter(|m|m.kind=="text").map(|m|{
            descriptors.iter().find(|d|d["model"]==m.model).cloned().unwrap_or_else(||json!({"id":m.model,"model":m.model,"name":m.model,"object":"model","api_backend":"responses"}))
        }).collect())
    }
}

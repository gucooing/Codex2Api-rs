use crate::{Result, Storage, StorageError, VirtualClientState};
use codex2api_common::ModelRule;
use serde_json::{Value, json};
use sqlx::{FromRow, Row, postgres::PgRow};

/// Plan rows with their supplier tag set; every plan read selects these columns.
pub(crate) const PLAN_COLUMNS: &str = "p.*,COALESCE((SELECT jsonb_agg(t.tag_id ORDER BY t.tag_id) FROM virtual_plan_tags t WHERE t.plan_id=p.id),'[]'::jsonb) AS tag_ids";

/// A plan belongs to one client platform; its model list and tag set may span platforms.
#[derive(Clone)]
pub struct VirtualPlan {
    pub provider_id: String,
    pub id: String,
    pub name: String,
    pub plan_type: String,
    pub config: Value,
    pub allow_purchase: bool,
    pub revision: i64,
    pub updated_at_ms: i64,
    pub tag_ids: Vec<String>,
}

impl<'r> FromRow<'r, PgRow> for VirtualPlan {
    fn from_row(row: &'r PgRow) -> std::result::Result<Self, sqlx::Error> {
        let config: sqlx::types::Json<Value> = row.try_get("config")?;
        let tag_ids: sqlx::types::Json<Vec<String>> = row.try_get("tag_ids")?;
        Ok(Self {
            provider_id: row.try_get("provider_id")?,
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            plan_type: row.try_get("plan_type")?,
            config: config.0,
            allow_purchase: row.try_get("allow_purchase")?,
            revision: row.try_get("revision")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
            tag_ids: tag_ids.0,
        })
    }
}

pub fn plan_owned_config(key: &str) -> bool {
    matches!(key, "quota" | "subscription_entitlements")
}

impl VirtualPlan {
    pub fn rpm_limit(&self) -> u32 {
        self.config["rpm_limit"].as_u64().unwrap_or(40) as u32
    }
    pub fn concurrency_limit(&self) -> u32 {
        self.config["concurrency_limit"].as_u64().unwrap_or(10) as u32
    }

    pub fn description(&self) -> &str {
        self.config["description"].as_str().unwrap_or("")
    }

    pub fn models(&self) -> Result<Vec<ModelRule>> {
        serde_json::from_value(self.config["models"].clone())
            .map_err(|_| StorageError::Constraint("套餐模型列表无效".into()))
    }
    pub fn model_rule(&self, model: &str) -> Option<ModelRule> {
        self.models()
            .ok()?
            .into_iter()
            .find(|rule| rule.model == model)
    }
    pub fn validate(&self) -> Result<()> {
        self.sale_price_cents()?;
        self.duration_days()?;
        if self
            .config
            .get("description")
            .is_some_and(|v| v.as_str().is_none_or(|s| s.chars().count() > 20_000))
        {
            return Err(StorageError::Constraint(
                "套餐描述须为文本，长度不能超过 20000 字符".into(),
            ));
        }
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err(StorageError::Constraint(
                "请填写套餐名称，长度不能超过 128 字节".into(),
            ));
        }
        let valid_tier =
            codex2api_common::valid_subscription_tier(&self.provider_id, &self.plan_type);
        if !valid_tier || self.plan_type == "free" {
            return Err(StorageError::InvalidAdminUpdate(
                "Invalid client subscription tier",
            ));
        }
        if !self.config.is_object() || !self.config["models"].is_array() {
            return Err(StorageError::Constraint("套餐模型列表无效".into()));
        }
        crate::api_groups::validate_model_rules(&self.models()?, 256)?;
        if self.tag_ids.len() > 100 {
            return Err(StorageError::Constraint("套餐标签不能超过 100 个".into()));
        }
        for key in ["rpm_limit", "concurrency_limit"] {
            if self
                .config
                .get(key)
                .is_some_and(|v| v.as_u64().is_none_or(|n| n > 1_000_000))
            {
                return Err(StorageError::InvalidAdminUpdate(
                    "RPM 与并发限制须为 0 到 1000000 的整数",
                ));
            }
        }
        crate::plan_spending_windows(&self.config)?;
        Ok(())
    }
}

impl Storage {
    /// Enabled catalog models of every platform, selectable for a plan.
    pub async fn virtual_plan_model_choices(&self) -> Result<Vec<String>> {
        Ok(self
            .model_configs()
            .await?
            .into_iter()
            .filter(|model| model.enabled)
            .map(|model| model.model)
            .collect())
    }

    pub async fn virtual_plans(&self) -> Result<Vec<VirtualPlan>> {
        Ok(sqlx::query_as(&format!(
            "SELECT {PLAN_COLUMNS} FROM virtual_plans p ORDER BY p.sort_id"
        ))
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn virtual_plan(&self, id: &str) -> Result<Option<VirtualPlan>> {
        Ok(sqlx::query_as(&format!(
            "SELECT {PLAN_COLUMNS} FROM virtual_plans p WHERE p.id=$1"
        ))
        .bind(id)
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn save_virtual_plan(
        &self,
        plan: &VirtualPlan,
        expected: Option<i64>,
    ) -> Result<bool> {
        plan.validate()?;
        let mut tx = self.pool().begin().await?;
        crate::api_groups::require_supplier_tags(&mut tx, &plan.tag_ids).await?;
        let models: Vec<String> = plan.models()?.into_iter().map(|r| r.model).collect();
        crate::api_groups::require_catalog_models(&mut tx, &models).await?;
        let now = chrono::Utc::now().timestamp_millis();
        let changed = if let Some(revision) = expected {
            sqlx::query("UPDATE virtual_plans SET name=$1,config=$2::jsonb,allow_purchase=$3,revision=revision+1,updated_at_ms=$4,plan_type=$5 WHERE id=$6 AND revision=$7 AND provider_id=$8")
                .bind(plan.name.trim()).bind(plan.config.to_string()).bind(plan.allow_purchase).bind(now).bind(&plan.plan_type).bind(&plan.id).bind(revision).bind(&plan.provider_id).execute(&mut *tx).await?.rows_affected()
        } else {
            sqlx::query("INSERT INTO virtual_plans(provider_id,id,name,plan_type,config,allow_purchase,updated_at_ms) VALUES($1,$2,$3,$4,$5::jsonb,$6,$7)")
                .bind(&plan.provider_id).bind(&plan.id).bind(plan.name.trim()).bind(&plan.plan_type).bind(plan.config.to_string()).bind(plan.allow_purchase).bind(now).execute(&mut *tx).await?.rows_affected()
        };
        if changed == 1 {
            sqlx::query("DELETE FROM virtual_plan_tags WHERE plan_id=$1")
                .bind(&plan.id)
                .execute(&mut *tx)
                .await?;
            for tag in plan
                .tag_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
            {
                sqlx::query("INSERT INTO virtual_plan_tags(plan_id,tag_id) VALUES($1,$2)")
                    .bind(&plan.id)
                    .bind(tag)
                    .execute(&mut *tx)
                    .await?;
            }
            sqlx::query(
                "UPDATE platform_accounts SET plan_type=$1 WHERE plan_id=$2 AND provider_id=$3",
            )
            .bind(&plan.plan_type)
            .bind(&plan.id)
            .bind(&plan.provider_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        if changed == 1 {
            self.runtime
                .snapshots
                .invalidate(&[crate::snapshots::Domain::Subscriptions]);
        }
        Ok(changed == 1)
    }

    pub async fn delete_virtual_plan(&self, id: &str, revision: i64) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        // Acquire the write lock before checking references, including concurrent assignments.
        let changed = sqlx::query("DELETE FROM virtual_plans WHERE id=$1 AND revision=$2 AND NOT EXISTS(SELECT 1 FROM platform_accounts WHERE plan_id=$3)")
            .bind(id).bind(revision).bind(id).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            let used: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM platform_accounts WHERE plan_id=$1)",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
            if used {
                return Err(StorageError::Constraint(
                    "套餐仍有账户使用，请先为这些账户更换套餐".into(),
                ));
            }
        }
        tx.commit().await?;
        Ok(changed == 1)
    }

    pub(crate) async fn virtual_plan_config(
        &self,
        owner: &str,
        key: &str,
    ) -> Result<VirtualClientState> {
        let account = self
            .platform_account(owner)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(owner.into()))?;
        let plan = self
            .virtual_plan(&account.plan_id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(owner.into()))?;
        let config = &plan.config;
        let windows = crate::plan_spending_windows(config)?;
        let value = match key {
            "quota" => json!({"spending_windows":windows}),
            "subscription_entitlements" => {
                let models: Vec<String> = plan.models()?.into_iter().map(|r| r.model).collect();
                json!({plan.plan_type:{"models":models,"spending_windows":windows}})
            }
            _ => {
                return Err(StorageError::InvalidAdminUpdate(
                    "Unknown plan configuration",
                ));
            }
        };
        Ok(VirtualClientState {
            value,
            revision: plan.revision,
            write_origin: "admin".into(),
            updated_at_ms: plan.updated_at_ms,
        })
    }
}

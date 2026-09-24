use crate::{Result, Storage, StorageError, VirtualClientState};
use serde_json::{Value, json};
use sqlx::{FromRow, Row, sqlite::SqliteRow};

#[derive(Clone)]
pub struct VirtualPlan {
    pub provider_id: String,
    pub id: String,
    pub name: String,
    pub plan_type: String,
    pub config: Value,
    pub enabled: bool,
    pub revision: i64,
    pub updated_at_ms: i64,
}

impl<'r> FromRow<'r, SqliteRow> for VirtualPlan {
    fn from_row(row: &'r SqliteRow) -> std::result::Result<Self, sqlx::Error> {
        let config: String = row.try_get("config")?;
        Ok(Self {
            provider_id: row.try_get("provider_id")?,
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            plan_type: row.try_get("plan_type")?,
            config: serde_json::from_str(&config).map_err(|e| sqlx::Error::ColumnDecode {
                index: "config".into(),
                source: Box::new(e),
            })?,
            enabled: row.try_get("enabled")?,
            revision: row.try_get("revision")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
        })
    }
}

pub fn plan_owned_config(key: &str) -> bool {
    matches!(
        key,
        "quota" | "subscription_entitlements" | "subscription_policy"
    )
}

impl VirtualPlan {
    pub fn model_access(&self, free_fallback: bool) -> Result<codex2api_core::ModelAccess> {
        let (mode, items) = if free_fallback {
            ("free_model_access", "free_models")
        } else {
            ("model_access", "models")
        };
        let models = self.config[items]
            .as_array()
            .ok_or_else(|| StorageError::Constraint("模型权限配置无效".into()))?;
        let models: Option<Vec<codex2api_core::ModelRef>> = models
            .iter()
            .map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
        codex2api_core::ModelAccess::from_config(
            self.config[mode].as_str().unwrap_or(""),
            models.ok_or_else(|| StorageError::Constraint("模型权限配置无效".into()))?,
        )
        .map_err(|_| StorageError::Constraint("请选择明确的模型权限范围".into()))
    }
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err(StorageError::Constraint(
                "请填写套餐名称，长度不能超过 128 字节".into(),
            ));
        }
        if !["free", "plus", "pro", "business", "enterprise", "edu"]
            .contains(&self.plan_type.as_str())
        {
            return Err(StorageError::InvalidAdminUpdate(
                "Invalid client subscription compatibility value",
            ));
        }
        let value = &self.config;
        let fields = [
            "models",
            "model_access",
            "free_models",
            "free_model_access",
            "free_access_enabled",
        ];
        if !value
            .as_object()
            .is_some_and(|v| fields.iter().all(|k| v.contains_key(*k)))
            || !value["free_access_enabled"].is_boolean()
            || !value["models"].as_array().is_some_and(|a| {
                a.len() <= 256
                    && a.iter().all(|v| {
                        serde_json::from_value::<codex2api_core::ModelRef>(v.clone()).is_ok_and(
                            |m| codex2api_core::valid_model(&m.model) && !m.provider_id.is_empty(),
                        )
                    })
            })
        {
            return Err(StorageError::Constraint(
                "套餐模型范围或费用额度无效；金额须为非负美元数，最多九位小数".into(),
            ));
        }
        for key in ["models", "free_models"] {
            if self.config[key]
                .as_array()
                .into_iter()
                .flatten()
                .any(|m| m["provider_id"] != self.provider_id)
            {
                return Err(StorageError::Constraint(
                    "套餐只能授权同一提供商的模型".into(),
                ));
            }
        }
        self.model_access(false)?;
        self.model_access(true)?;
        crate::plan_spending_windows(value, false)?;
        crate::plan_spending_windows(value, true)?;
        Ok(())
    }
}

impl Storage {
    /// Models already configured for billing or observed by this service.
    pub async fn virtual_plan_model_choices(&self, provider: &str) -> Result<Vec<String>> {
        Ok(self
            .model_configs(provider)
            .await?
            .into_iter()
            .filter(|model| model.enabled)
            .map(|model| model.model)
            .collect())
    }

    pub async fn virtual_plans(&self) -> Result<Vec<VirtualPlan>> {
        Ok(sqlx::query_as("SELECT * FROM virtual_plans ORDER BY rowid")
            .fetch_all(self.pool())
            .await?)
    }

    pub async fn virtual_plan(&self, id: &str) -> Result<Option<VirtualPlan>> {
        Ok(sqlx::query_as("SELECT * FROM virtual_plans WHERE id=?")
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
        let now = chrono::Utc::now().timestamp_millis();
        let changed = if let Some(revision) = expected {
            // A plan's protocol type is fixed; change an account's subscription by selecting another plan.
            sqlx::query("UPDATE virtual_plans SET name=?,config=?,enabled=?,revision=revision+1,updated_at_ms=? WHERE id=? AND revision=? AND plan_type=? AND provider_id=?")
                .bind(plan.name.trim()).bind(plan.config.to_string()).bind(plan.enabled).bind(now).bind(&plan.id).bind(revision).bind(&plan.plan_type).bind(&plan.provider_id).execute(self.pool()).await?.rows_affected()
        } else {
            sqlx::query("INSERT INTO virtual_plans(provider_id,id,name,plan_type,config,enabled,updated_at_ms) VALUES(?,?,?,?,?,?,?)")
                .bind(&plan.provider_id).bind(&plan.id).bind(plan.name.trim()).bind(&plan.plan_type).bind(plan.config.to_string()).bind(plan.enabled).bind(now).execute(self.pool()).await?.rows_affected()
        };
        Ok(changed == 1)
    }

    pub async fn delete_virtual_plan(&self, id: &str, revision: i64) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        // Acquire the write lock before checking references, including concurrent assignments.
        let changed = sqlx::query("DELETE FROM virtual_plans WHERE id=? AND revision=? AND NOT EXISTS(SELECT 1 FROM virtual_accounts WHERE plan_id=?)")
            .bind(id).bind(revision).bind(id).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            let used: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM virtual_accounts WHERE plan_id=?)")
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
        let plan: VirtualPlan = sqlx::query_as("SELECT p.* FROM virtual_plans p JOIN virtual_accounts a ON a.plan_id=p.id WHERE a.id=?")
            .bind(owner).fetch_optional(self.pool()).await?.ok_or_else(||StorageError::AccountNotFound(owner.into()))?;
        let config = &plan.config;
        let quota = if config.get("primary_cost_limit_usd").is_some()
            || config.get("weekly_cost_limit_usd").is_some()
        {
            json!({"primary_cost_limit_usd":config["primary_cost_limit_usd"],"weekly_cost_limit_usd":config["weekly_cost_limit_usd"]})
        } else {
            json!({"spending_windows":config["spending_windows"]})
        };
        let primary_key = if plan.plan_type == "free" {
            "primary_cost_limit_usd"
        } else {
            "free_primary_cost_limit_usd"
        };
        let weekly_key = if plan.plan_type == "free" {
            "weekly_cost_limit_usd"
        } else {
            "free_weekly_cost_limit_usd"
        };
        let nested_key = if plan.plan_type == "free" {
            "spending_windows"
        } else {
            "free_spending_windows"
        };
        let value = match key {
            "quota" => quota,
            "subscription_policy" => {
                if config.get(primary_key).is_some() {
                    json!({"free_access_enabled":config["free_access_enabled"],
                        "primary_cost_limit_usd":config[primary_key],
                        "weekly_cost_limit_usd":config[weekly_key]})
                } else {
                    json!({"free_access_enabled":config["free_access_enabled"],
                        "spending_windows":config[nested_key]})
                }
            }
            "subscription_entitlements" => {
                if config.get("primary_cost_limit_usd").is_some() {
                    json!({plan.plan_type:{"models":config["models"],"primary_cost_limit_usd":config["primary_cost_limit_usd"],"weekly_cost_limit_usd":config["weekly_cost_limit_usd"]}})
                } else {
                    json!({plan.plan_type:{"models":config["models"],"spending_windows":config["spending_windows"]}})
                }
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

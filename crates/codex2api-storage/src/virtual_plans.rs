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
    pub allow_purchase: bool,
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
            allow_purchase: row.try_get("allow_purchase")?,
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
    pub fn description(&self) -> &str {
        self.config["description"].as_str().unwrap_or("")
    }

    pub fn model_access(&self) -> Result<codex2api_core::ModelAccess> {
        let (mode, items) = ("model_access", "models");
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
        if self.plan_type == "free" && self.allow_purchase {
            return Err(StorageError::InvalidAdminUpdate(
                "Free 套餐自动提供，无需开放购买",
            ));
        }
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err(StorageError::Constraint(
                "请填写套餐名称，长度不能超过 128 字节".into(),
            ));
        }
        let valid_tier =
            codex2api_core::valid_subscription_tier(&self.provider_id, &self.plan_type);
        if !valid_tier {
            return Err(StorageError::InvalidAdminUpdate(
                "Invalid client subscription compatibility value",
            ));
        }
        let value = &self.config;
        let fields = ["models", "model_access"];
        if !value
            .as_object()
            .is_some_and(|v| fields.iter().all(|k| v.contains_key(*k)))
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
        for key in ["models"] {
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
        self.model_access()?;
        crate::plan_spending_windows(value)?;
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
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        if let Some(tag) = plan.config["supplier_tag_id"].as_str() {
            let valid: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM supplier_tags WHERE id=? AND provider_id=?)",
            )
            .bind(tag)
            .bind(&plan.provider_id)
            .fetch_one(&mut *tx)
            .await?;
            if !valid {
                return Err(StorageError::InvalidAdminUpdate("请选择同平台的供应号池"));
            }
        }
        let now = chrono::Utc::now().timestamp_millis();
        let free: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM virtual_plans WHERE id=? AND plan_type='free')",
        )
        .bind(&plan.id)
        .fetch_one(&mut *tx)
        .await?;
        if free && plan.plan_type != "free" {
            return Err(StorageError::InvalidAdminUpdate(
                "Free 套餐的订阅档位不能修改",
            ));
        }
        let changed = if let Some(revision) = expected {
            sqlx::query("UPDATE virtual_plans SET name=?,config=?,allow_purchase=?,revision=revision+1,updated_at_ms=?,plan_type=? WHERE id=? AND revision=? AND provider_id=?")
                .bind(plan.name.trim()).bind(plan.config.to_string()).bind(plan.allow_purchase).bind(now).bind(&plan.plan_type).bind(&plan.id).bind(revision).bind(&plan.provider_id).execute(&mut *tx).await?.rows_affected()
        } else {
            sqlx::query("INSERT INTO virtual_plans(provider_id,id,name,plan_type,config,allow_purchase,updated_at_ms) VALUES(?,?,?,?,?,?,?)")
                .bind(&plan.provider_id).bind(&plan.id).bind(plan.name.trim()).bind(&plan.plan_type).bind(plan.config.to_string()).bind(plan.allow_purchase).bind(now).execute(&mut *tx).await?.rows_affected()
        };
        if changed == 1 {
            sqlx::query(
                "UPDATE platform_accounts SET plan_type=? WHERE plan_id=? AND provider_id=?",
            )
            .bind(&plan.plan_type)
            .bind(&plan.id)
            .bind(&plan.provider_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(changed == 1)
    }

    pub async fn delete_virtual_plan(&self, id: &str, revision: i64) -> Result<bool> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let free: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM virtual_plans WHERE id=? AND plan_type='free')",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if free {
            return Err(StorageError::InvalidAdminUpdate("Free 套餐不能删除"));
        }
        // Acquire the write lock before checking references, including concurrent assignments.
        let changed = sqlx::query("DELETE FROM virtual_plans WHERE id=? AND revision=? AND NOT EXISTS(SELECT 1 FROM platform_accounts WHERE plan_id=?)")
            .bind(id).bind(revision).bind(id).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            let used: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM platform_accounts WHERE plan_id=?)",
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
            .effective_platform_account(owner)
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
            "subscription_policy" => {
                let free = self.platform_free_plan(&account.provider_id).await?;
                json!({"plan_id":free.id,"model_access":free.config["model_access"],"models":free.config["models"],"spending_windows":crate::plan_spending_windows(&free.config)?})
            }
            "subscription_entitlements" => {
                json!({plan.plan_type:{"models":config["models"],"spending_windows":windows}})
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

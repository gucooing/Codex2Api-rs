use crate::{Result, Storage, StorageError};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VirtualRpmLimit {
    pub rpm: Option<u32>,
    pub plan_rpm: u32,
    pub effective_rpm: u32,
    pub concurrency: Option<u32>,
    pub plan_concurrency: u32,
    pub effective_concurrency: u32,
}
impl Storage {
    /// Request-local proof; policy edits, settlements and time boundaries require re-admission.
    pub fn admission_version(&self, owner: &str) -> (u64, u64, i64) {
        (
            self.runtime.snapshots.policy_revision(),
            self.runtime
                .snapshots
                .version(&format!("spending-decision:{owner}")),
            chrono::Utc::now().timestamp(),
        )
    }
    pub async fn virtual_rpm_limit(&self, owner: &str) -> Result<VirtualRpmLimit> {
        let key = format!("request-limits:{owner}");
        self.runtime.snapshots.read(key.clone(), async {
        let (rpm,concurrency):(Option<i64>,Option<i64>)=sqlx::query_as("SELECT (SELECT rpm FROM virtual_rpm_limits WHERE virtual_account_id=$1),(SELECT concurrency FROM virtual_concurrency_limits WHERE virtual_account_id=$2)").bind(owner).bind(owner).fetch_one(self.pool()).await?;
        let account = self
            .platform_account(owner)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(owner.into()))?;
        let plan = self
            .virtual_plan(&account.plan_id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(account.plan_id.clone()))?;
        let rpm = rpm.map(|v| v as u32);
        let concurrency = concurrency.map(|v| v as u32);
        let value = VirtualRpmLimit {
            rpm,
            plan_rpm: plan.rpm_limit(),
            effective_rpm: rpm.unwrap_or(plan.rpm_limit()),
            concurrency,
            plan_concurrency: plan.concurrency_limit(),
            effective_concurrency: concurrency.unwrap_or(plan.concurrency_limit()),
        };
        Ok((value, std::time::Duration::from_secs(30)))
        }).await
    }
    pub async fn save_virtual_request_limits(
        &self,
        owner: &str,
        rpm: Option<u32>,
        concurrency: Option<u32>,
    ) -> Result<()> {
        if rpm.is_some_and(|v| v > 1_000_000) || concurrency.is_some_and(|v| v > 1_000_000) {
            return Err(StorageError::Constraint(
                "RPM 与并发限制须为 0 到 1000000 的整数".into(),
            ));
        }
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_subscription(&mut tx, owner).await?;
        for (table, column, value) in [
            ("virtual_rpm_limits", "rpm", rpm),
            ("virtual_concurrency_limits", "concurrency", concurrency),
        ] {
            if let Some(value) = value {
                sqlx::query(&format!("INSERT INTO {table}(virtual_account_id,{column}) VALUES($1,$2) ON CONFLICT(virtual_account_id) DO UPDATE SET {column}=excluded.{column}")).bind(owner).bind(i64::from(value)).execute(&mut *tx).await?;
            } else {
                sqlx::query(&format!("DELETE FROM {table} WHERE virtual_account_id=$1"))
                    .bind(owner)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::Subscriptions]);
        Ok(())
    }
    pub async fn save_virtual_rpm_limit(&self, owner: &str, rpm: Option<u32>) -> Result<()> {
        self.save_virtual_request_limits(
            owner,
            rpm,
            self.virtual_rpm_limit(owner).await?.concurrency,
        )
        .await
    }
    fn model_scope(owner: &str, model: &str) -> String {
        format!("{owner}:model:{:x}", Sha256::digest(model.as_bytes()))
    }
    /// The subscription window and, when the plan limits the model, its model window
    /// are checked together before either consumes a slot.
    pub async fn admit_virtual_request(
        &self,
        owner: &str,
        model: Option<&str>,
        consume: bool,
    ) -> Result<Option<u32>> {
        let mut scopes = vec![(
            format!("rpm:{owner}"),
            self.virtual_rpm_limit(owner).await?.effective_rpm,
        )];
        if let Some(model) = model {
            let limit = self
                .effective_entitlements(owner)
                .await?
                .model_rule(model)
                .map_or(0, |rule| rule.rpm_limit);
            if limit > 0 {
                scopes.push((format!("rpm:{}", Self::model_scope(owner, model)), limit));
            }
        }
        self.runtime.rpm_scopes(&scopes, consume).await
    }
    /// One generation slot of the subscription and, when the plan limits the model,
    /// of that model. Dropping the permits releases both.
    pub async fn acquire_consumer(
        &self,
        id: &str,
        model: Option<&str>,
    ) -> Result<Option<Vec<crate::RuntimePermit>>> {
        let limits = self.virtual_rpm_limit(id).await?;
        let Some(permit) = self
            .runtime
            .acquire(&format!("consumer:{id}"), limits.effective_concurrency)
            .await?
        else {
            return Ok(None);
        };
        let mut permits = vec![permit];
        if let Some(model) = model {
            let limit = self
                .effective_entitlements(id)
                .await?
                .model_rule(model)
                .map_or(0, |rule| rule.concurrency_limit);
            if limit > 0 {
                let Some(permit) = self
                    .runtime
                    .acquire(&format!("consumer:{}", Self::model_scope(id, model)), limit)
                    .await?
                else {
                    return Ok(None);
                };
                permits.push(permit);
            }
        }
        Ok(Some(permits))
    }
    pub fn consumer_active(&self, id: &str) -> u32 {
        self.runtime.metrics(&format!("consumer:{id}")).active
    }
}

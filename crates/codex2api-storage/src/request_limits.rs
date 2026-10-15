use crate::{Result, Storage, StorageError};
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
    pub async fn admit_virtual_request(&self, owner: &str, consume: bool) -> Result<Option<u32>> {
        self.runtime
            .rpm(
                owner,
                self.virtual_rpm_limit(owner).await?.effective_rpm,
                consume,
            )
            .await
    }
}

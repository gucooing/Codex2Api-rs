use crate::{Result, Storage, StorageError};

#[derive(Debug, serde::Serialize)]
pub struct VirtualRpmLimit {
    pub rpm: Option<u32>,
    pub default_rpm: u32,
    pub effective_rpm: u32,
}

impl Storage {
    pub async fn virtual_rpm_limit(&self, owner: &str) -> Result<VirtualRpmLimit> {
        let rpm: Option<i64> =
            sqlx::query_scalar("SELECT rpm FROM virtual_rpm_limits WHERE virtual_account_id=?")
                .bind(owner)
                .fetch_optional(self.pool())
                .await?;
        let default_rpm = self.gateway_settings().await?.default_rpm;
        let rpm = rpm.map(|n| n as u32);
        Ok(VirtualRpmLimit {
            rpm,
            default_rpm,
            effective_rpm: rpm.unwrap_or(default_rpm),
        })
    }

    pub async fn save_virtual_rpm_limit(&self, owner: &str, rpm: Option<u32>) -> Result<()> {
        if rpm.is_some_and(|n| n > 1_000_000) {
            return Err(StorageError::Constraint(
                "RPM 须为 0 到 1000000 的整数".into(),
            ));
        }
        if let Some(rpm) = rpm {
            sqlx::query("INSERT INTO virtual_rpm_limits(virtual_account_id,rpm) VALUES(?,?) ON CONFLICT(virtual_account_id) DO UPDATE SET rpm=excluded.rpm")
                .bind(owner).bind(i64::from(rpm)).execute(self.pool()).await?;
        } else {
            sqlx::query("DELETE FROM virtual_rpm_limits WHERE virtual_account_id=?")
                .bind(owner)
                .execute(self.pool())
                .await?;
        }
        Ok(())
    }

    /// Sliding sixty-second admission window, shared across devices/transports.
    /// Returns a retry delay in seconds. Peeks never consume admission slots.
    pub async fn admit_virtual_request(
        &self,
        owner: &str,
        consume: bool,
        now_ms: i64,
    ) -> Result<Option<u32>> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("DELETE FROM virtual_request_admissions WHERE admitted_at_ms<=?")
            .bind(now_ms - 60_000)
            .execute(&mut *tx)
            .await?;
        let settings: Option<String> =
            sqlx::query_scalar("SELECT value FROM meta WHERE key='gateway_settings'")
                .fetch_optional(&mut *tx)
                .await?;
        let default = settings
            .map(|v| serde_json::from_str::<crate::GatewaySettings>(&v))
            .transpose()?
            .unwrap_or_default()
            .default_rpm;
        let rpm: Option<i64> =
            sqlx::query_scalar("SELECT rpm FROM virtual_rpm_limits WHERE virtual_account_id=?")
                .bind(owner)
                .fetch_optional(&mut *tx)
                .await?;
        let rpm = rpm.unwrap_or(i64::from(default));
        if rpm == 0 {
            tx.commit().await?;
            return Ok(None);
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM virtual_request_admissions WHERE virtual_account_id=?",
        )
        .bind(owner)
        .fetch_one(&mut *tx)
        .await?;
        if count >= rpm {
            // Also handles administrators lowering the limit during a window.
            let expiry: i64 = sqlx::query_scalar("SELECT admitted_at_ms+60000 FROM virtual_request_admissions WHERE virtual_account_id=? ORDER BY admitted_at_ms,id LIMIT 1 OFFSET ?")
                .bind(owner).bind(count-rpm).fetch_one(&mut *tx).await?;
            tx.commit().await?;
            return Ok(Some(((expiry - now_ms + 999) / 1000).clamp(1, 60) as u32));
        }
        if consume {
            sqlx::query("INSERT INTO virtual_request_admissions(virtual_account_id,admitted_at_ms) VALUES(?,?)")
                .bind(owner).bind(now_ms).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(None)
    }
}

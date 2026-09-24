use crate::{Result, Storage};
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct QuotaSnapshot {
    pub value: Value,
    pub observed_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SupplierInfoSection {
    Quota,
    Usage,
    Details,
    Credits,
}
impl SupplierInfoSection {
    pub const ALL: [Self; 4] = [Self::Quota, Self::Usage, Self::Details, Self::Credits];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quota => "quota",
            Self::Usage => "usage",
            Self::Details => "details",
            Self::Credits => "credits",
        }
    }
}

impl Storage {
    pub async fn get_supplier_info(
        &self,
        account_id: &str,
        section: SupplierInfoSection,
    ) -> Result<Option<QuotaSnapshot>> {
        if section == SupplierInfoSection::Quota {
            return self.get_account_quota(account_id).await;
        }
        let row: Option<(String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT response_json, observed_at FROM supplier_info_cache WHERE account_id = ? AND section = ?",
        ).bind(account_id).bind(section.as_str()).fetch_optional(self.pool()).await?;
        row.map(|(raw, observed_at)| {
            Ok(QuotaSnapshot {
                value: serde_json::from_str(&raw)?,
                observed_at,
            })
        })
        .transpose()
    }

    pub async fn store_supplier_info(
        &self,
        account_id: &str,
        section: SupplierInfoSection,
        snapshot: &QuotaSnapshot,
    ) -> Result<()> {
        if section == SupplierInfoSection::Quota {
            return self.store_account_quota(account_id, snapshot).await;
        }
        sqlx::query("INSERT INTO supplier_info_cache(account_id, section, response_json, observed_at) VALUES(?, ?, ?, ?) ON CONFLICT(account_id, section) DO UPDATE SET response_json=excluded.response_json, observed_at=excluded.observed_at")
            .bind(account_id).bind(section.as_str()).bind(serde_json::to_string(&snapshot.value)?).bind(snapshot.observed_at).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn delete_supplier_info(
        &self,
        account_id: &str,
        section: SupplierInfoSection,
    ) -> Result<()> {
        if section == SupplierInfoSection::Quota {
            return self.delete_account_quota(account_id).await;
        }
        sqlx::query("DELETE FROM supplier_info_cache WHERE account_id = ? AND section = ?")
            .bind(account_id)
            .bind(section.as_str())
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn get_account_quota(&self, account_id: &str) -> Result<Option<QuotaSnapshot>> {
        let row = sqlx::query_as::<_, (String, DateTime<Utc>)>(
            "SELECT response_json, observed_at FROM account_quota_cache WHERE account_id = ?",
        )
        .bind(account_id)
        .fetch_optional(self.pool())
        .await?;
        row.map(|(json, observed_at)| {
            Ok(QuotaSnapshot {
                value: serde_json::from_str(&json)?,
                observed_at,
            })
        })
        .transpose()
    }

    pub async fn store_account_quota(
        &self,
        account_id: &str,
        snapshot: &QuotaSnapshot,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO account_quota_cache (account_id, response_json, observed_at) VALUES (?, ?, ?)
             ON CONFLICT(account_id) DO UPDATE SET
                 response_json = excluded.response_json, observed_at = excluded.observed_at",
        )
        .bind(account_id)
        .bind(serde_json::to_string(&snapshot.value)?)
        .bind(snapshot.observed_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_account_quota(&self, account_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM account_quota_cache WHERE account_id = ?")
            .bind(account_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

use crate::{Result, Storage};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, Default, FromRow, Serialize)]
pub struct SupplierHealth {
    pub error_message: Option<String>,
    pub error_at: Option<String>,
    pub revision: i64,
    pub authentication_invalid: bool,
}

impl Storage {
    pub async fn supplier_health(&self, id: &str) -> Result<SupplierHealth> {
        Ok(sqlx::query_as(
            "SELECT h.error_message,h.error_at,h.revision,
             COALESCE(h.rejected_auth_revision=a.auth_revision,0) AS authentication_invalid
             FROM supplier_health h JOIN supplier_accounts a ON a.id=h.account_id WHERE h.account_id=?",
        ).bind(id).fetch_optional(self.pool()).await?.unwrap_or_default())
    }

    /// The caller must have observed an upstream 401 for this credential revision.
    /// Late failures cannot reject credentials that have since been refreshed/replaced.
    pub async fn reject_supplier_auth(&self, id: &str, auth_revision: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO supplier_health(account_id,error_message,error_at,revision,rejected_auth_revision)
             SELECT id,'ChatGPT 授权失效（401），请重新授权或检查恢复',strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,auth_revision
             FROM supplier_accounts WHERE id=? AND auth_revision=?
             ON CONFLICT(account_id) DO UPDATE SET error_message=excluded.error_message,
             error_at=excluded.error_at,revision=supplier_health.revision+1,rejected_auth_revision=excluded.rejected_auth_revision",
        ).bind(id).bind(auth_revision).execute(self.pool()).await?;
        Ok(())
    }

    /// A successful check cannot erase a newer rejection or change manual enablement.
    pub async fn recover_supplier(&self, id: &str, revision: i64) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE supplier_health SET error_message=NULL,error_at=NULL,revision=revision+1,rejected_auth_revision=NULL
             WHERE account_id=? AND revision=? AND error_message IS NOT NULL",
        ).bind(id).bind(revision).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }
}

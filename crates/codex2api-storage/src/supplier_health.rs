use crate::{Result, Storage};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, Default, FromRow, Serialize)]
pub struct SupplierHealth {
    pub error_message: Option<String>,
    pub error_at: Option<String>,
    pub revision: i64,
    pub authentication_invalid: bool,
    pub cooldown_kind: Option<String>,
    pub cooldown_until: Option<i64>,
    pub cooldown_code: Option<String>,
}

impl Storage {
    pub async fn supplier_health(&self, id: &str) -> Result<SupplierHealth> {
        Ok(sqlx::query_as(
            "SELECT h.error_message,h.error_at,h.revision,
             CASE WHEN h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision AND h.cooldown_until>unixepoch() THEN h.cooldown_kind END AS cooldown_kind,
             CASE WHEN h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision AND h.cooldown_until>unixepoch() THEN h.cooldown_until END AS cooldown_until,
             h.cooldown_code,
             COALESCE(h.rejected_auth_revision=a.auth_revision,0) AS authentication_invalid
             FROM supplier_health h JOIN supplier_accounts a ON a.id=h.account_id WHERE h.account_id=?",
        ).bind(id).fetch_optional(self.pool()).await?.unwrap_or_default())
    }

    /// Observations carry the credential revision used for that attempt; late
    /// failures cannot cool down freshly replaced authorization.
    pub async fn exhaust_supplier_quota(
        &self,
        id: &str,
        auth_revision: i64,
        until: i64,
        code: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO supplier_health(account_id,cooldown_kind,cooldown_until,cooldown_auth_revision,cooldown_code,cooldown_observed_at)
            SELECT id,'quota_exhausted',?,auth_revision,?,unixepoch() FROM supplier_accounts WHERE id=? AND auth_revision=?
            ON CONFLICT(account_id) DO UPDATE SET
            cooldown_kind=excluded.cooldown_kind,
            cooldown_until=CASE WHEN supplier_health.cooldown_kind='quota_exhausted' AND supplier_health.cooldown_auth_revision=excluded.cooldown_auth_revision THEN MAX(COALESCE(supplier_health.cooldown_until,0),excluded.cooldown_until) ELSE excluded.cooldown_until END,
            cooldown_auth_revision=excluded.cooldown_auth_revision,cooldown_code=excluded.cooldown_code,cooldown_observed_at=excluded.cooldown_observed_at")
            .bind(until).bind(code).bind(id).bind(auth_revision).execute(self.pool()).await?;
        self.refresh_supplier_bindings(id).await?;
        Ok(())
    }

    /// The caller must have observed an upstream 401 for this credential revision.
    /// Late failures cannot reject credentials that have since been refreshed/replaced.
    pub async fn reject_supplier_auth(&self, id: &str, auth_revision: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO supplier_health(account_id,error_message,error_at,revision,rejected_auth_revision)
             SELECT id,'供应账户授权已失效，请重新授权或检查恢复',strftime('%Y-%m-%dT%H:%M:%fZ','now'),1,auth_revision
             FROM supplier_accounts WHERE id=? AND auth_revision=?
             ON CONFLICT(account_id) DO UPDATE SET error_message=excluded.error_message,
             error_at=excluded.error_at,revision=supplier_health.revision+1,rejected_auth_revision=excluded.rejected_auth_revision",
        ).bind(id).bind(auth_revision).execute(self.pool()).await?;
        self.refresh_supplier_bindings(id).await?;
        Ok(())
    }

    /// A successful check cannot erase a newer rejection or change manual enablement.
    pub async fn recover_supplier(&self, id: &str, revision: i64) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE supplier_health SET error_message=NULL,error_at=NULL,revision=revision+1,rejected_auth_revision=NULL
             WHERE account_id=? AND revision=? AND error_message IS NOT NULL",
        ).bind(id).bind(revision).execute(self.pool()).await?;
        let recovered = result.rows_affected() == 1;
        if recovered {
            self.refresh_supplier_bindings(id).await?;
        }
        Ok(recovered)
    }
}

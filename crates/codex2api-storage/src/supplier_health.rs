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
    pub cooldown_revision: i64,
}

impl Storage {
    pub async fn supplier_health(&self, id: &str) -> Result<SupplierHealth> {
        Ok(sqlx::query_as(
            "SELECT h.error_message,h.error_at,h.revision,
             CASE WHEN h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision AND h.cooldown_until>unixepoch() THEN h.cooldown_kind END AS cooldown_kind,
             CASE WHEN h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision AND h.cooldown_until>unixepoch() THEN h.cooldown_until END AS cooldown_until,
             h.cooldown_code,h.cooldown_revision,
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
        self.record_supplier_quota_exhaustion(id, auth_revision, until, code, None)
            .await
    }

    pub async fn exhaust_supplier_quota_if_unchanged(
        &self,
        id: &str,
        auth_revision: i64,
        until: i64,
        code: &str,
        cooldown_revision: i64,
    ) -> Result<()> {
        self.record_supplier_quota_exhaustion(
            id,
            auth_revision,
            until,
            code,
            Some(cooldown_revision),
        )
        .await
    }

    async fn record_supplier_quota_exhaustion(
        &self,
        id: &str,
        auth_revision: i64,
        until: i64,
        code: &str,
        expected: Option<i64>,
    ) -> Result<()> {
        sqlx::query("INSERT INTO supplier_health(account_id,cooldown_kind,cooldown_until,cooldown_auth_revision,cooldown_code,cooldown_observed_at,cooldown_revision)
            SELECT id,'quota_exhausted',?,auth_revision,?,unixepoch(),1 FROM supplier_accounts a WHERE id=? AND auth_revision=?
            AND (? IS NULL OR COALESCE((SELECT cooldown_revision FROM supplier_health WHERE account_id=a.id),0)=?)
            ON CONFLICT(account_id) DO UPDATE SET
            cooldown_kind=excluded.cooldown_kind,
            cooldown_until=CASE WHEN supplier_health.cooldown_kind='quota_exhausted' AND supplier_health.cooldown_auth_revision=excluded.cooldown_auth_revision THEN MAX(COALESCE(supplier_health.cooldown_until,0),excluded.cooldown_until) ELSE excluded.cooldown_until END,
            cooldown_auth_revision=excluded.cooldown_auth_revision,cooldown_code=excluded.cooldown_code,cooldown_observed_at=excluded.cooldown_observed_at,cooldown_revision=supplier_health.cooldown_revision+1")
            .bind(until).bind(code).bind(id).bind(auth_revision).bind(expected).bind(expected).execute(self.pool()).await?;
        self.refresh_supplier_bindings(id).await?;
        Ok(())
    }

    /// Administrator-requested retry, independent of provider reachability.
    /// Only quota state is cleared; credential rejection and manual enablement
    /// continue to govern eligibility. In-flight quota checks lose their revision.
    pub async fn reset_supplier_quota(&self, id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO supplier_health(account_id,cooldown_revision)
             SELECT id,1 FROM supplier_accounts WHERE id=?
             ON CONFLICT(account_id) DO UPDATE SET cooldown_kind=NULL,cooldown_until=NULL,
             cooldown_auth_revision=NULL,cooldown_code=NULL,cooldown_observed_at=NULL,
             cooldown_revision=supplier_health.cooldown_revision+1",
        )
        .bind(id)
        .execute(self.pool())
        .await?;
        self.refresh_supplier_bindings(id).await?;
        Ok(())
    }

    /// A fresh official quota check may recover before the previously reported
    /// reset. Never erase an exhaustion observed after the check started.
    pub async fn recover_supplier_quota(
        &self,
        id: &str,
        auth_revision: i64,
        cooldown_revision: i64,
    ) -> Result<bool> {
        let changed = sqlx::query(
            "UPDATE supplier_health SET cooldown_kind=NULL,cooldown_until=NULL,
             cooldown_auth_revision=NULL,cooldown_code=NULL,cooldown_observed_at=NULL,
             cooldown_revision=cooldown_revision+1
             WHERE account_id=? AND cooldown_kind='quota_exhausted'
             AND cooldown_auth_revision=? AND cooldown_revision=?
             AND EXISTS(SELECT 1 FROM supplier_accounts WHERE id=? AND auth_revision=?)",
        )
        .bind(id)
        .bind(auth_revision)
        .bind(cooldown_revision)
        .bind(id)
        .bind(auth_revision)
        .execute(self.pool())
        .await?
        .rows_affected()
            == 1;
        if changed {
            self.refresh_supplier_bindings(id).await?;
        }
        Ok(changed)
    }

    /// Only currently exhausted suppliers are probed. Manual disablement,
    /// rejected credentials and obsolete credential revisions are excluded.
    pub async fn supplier_quota_probe_candidates(&self, provider: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT a.id FROM supplier_accounts a
             JOIN supplier_health h ON h.account_id=a.id
             JOIN supplier_tokens t ON t.account_id=a.id
             WHERE a.provider_id=? AND a.status='active' AND COALESCE(t.access_token,'')!=''
             AND h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision
             AND h.cooldown_until>unixepoch()
             AND (h.rejected_auth_revision IS NULL OR h.rejected_auth_revision!=a.auth_revision)
             ORDER BY h.cooldown_observed_at,a.id",
        )
        .bind(provider)
        .fetch_all(self.pool())
        .await?)
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

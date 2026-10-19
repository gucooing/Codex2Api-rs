use crate::{Result, Storage};
use serde::Serialize;
use sqlx::FromRow;

/// The models sharing a quota with model `model` of account `account` (SQL
/// placeholders): the model itself and every model of that account whose descriptor
/// names the same `quota_pool`, so one quota's models cool down and recover together.
fn pool_models(model: &str, account: &str) -> String {
    format!(
        "SELECT {model}::text AS model UNION SELECT m.model FROM supplier_models m
         WHERE m.account_id={account} AND m.descriptor_json::jsonb->>'quota_pool'=(
             SELECT s.descriptor_json::jsonb->>'quota_pool' FROM supplier_models s
             WHERE s.account_id={account} AND s.model={model})"
    )
}

#[derive(Debug, Clone, Default, FromRow, Serialize)]
pub struct SupplierHealth {
    pub error_message: Option<String>,
    pub error_at: Option<String>,
    pub revision: i64,
    pub authentication_invalid: bool,
    pub payment_required: bool,
    pub payment_required_at: Option<String>,
    pub payment_required_code: Option<String>,
    pub cooldown_kind: Option<String>,
    pub cooldown_until: Option<i64>,
    pub cooldown_code: Option<String>,
    pub cooldown_revision: i64,
}

/// A model whose quota is exhausted on one account while its other models stay eligible.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SupplierModelCooldown {
    pub model: String,
    pub cooldown_until: i64,
    pub cooldown_code: Option<String>,
}

impl SupplierHealth {
    pub fn display_status(&self, enabled: bool, authorized: bool) -> &'static str {
        if !enabled {
            "disabled"
        } else if !authorized || self.authentication_invalid {
            "error"
        } else if self.payment_required {
            "payment_required"
        } else if self.cooldown_kind.as_deref() == Some("quota_exhausted") {
            "quota_exhausted"
        } else {
            "active"
        }
    }
}

/// A successful explicit login starts a new authorization generation even if
/// the provider returns identical token bytes. It never changes manual enablement.
pub(crate) async fn complete_authorization(
    conn: &mut sqlx::PgConnection,
    id: &str,
    previous_revision: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=$1 AND auth_revision=$2",
    )
    .bind(id)
    .bind(previous_revision)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE supplier_health SET error_message=NULL,error_at=NULL,rejected_auth_revision=NULL,
        revision=revision+1,payment_required_at=NULL,payment_required_code=NULL,
        cooldown_kind=NULL,cooldown_until=NULL,cooldown_auth_revision=NULL,cooldown_code=NULL,
        cooldown_observed_at=NULL,cooldown_revision=cooldown_revision+1 WHERE account_id=$1",
    )
    .bind(id)
    .execute(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM supplier_model_cooldowns WHERE account_id=$1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

impl Storage {
    pub async fn supplier_health(&self, id: &str) -> Result<SupplierHealth> {
        Ok(sqlx::query_as(
            "SELECT h.error_message,h.error_at,h.revision,
             h.payment_required_at IS NOT NULL AS payment_required,h.payment_required_at,h.payment_required_code,
             CASE WHEN h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision AND h.cooldown_until>floor(extract(epoch FROM clock_timestamp()))::bigint THEN h.cooldown_kind END AS cooldown_kind,
             CASE WHEN h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision AND h.cooldown_until>floor(extract(epoch FROM clock_timestamp()))::bigint THEN h.cooldown_until END AS cooldown_until,
             h.cooldown_code,h.cooldown_revision,
             COALESCE(h.rejected_auth_revision=a.auth_revision,FALSE) AS authentication_invalid
             FROM supplier_health h JOIN supplier_accounts a ON a.id=h.account_id WHERE h.account_id=$1",
        ).bind(id).fetch_optional(self.pool()).await?.unwrap_or_default())
    }

    pub async fn mark_supplier_payment_required(
        &self,
        id: &str,
        auth_revision: i64,
        code: Option<&str>,
        expected_observation: Option<i64>,
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_supplier(&mut tx, id).await?;
        sqlx::query("INSERT INTO supplier_health(account_id,payment_required_at,payment_required_code,cooldown_revision)
            SELECT id,to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'),$1,1 FROM supplier_accounts a
            WHERE id=$2 AND auth_revision=$3
            AND ($4 IS NULL OR COALESCE((SELECT cooldown_revision FROM supplier_health WHERE account_id=a.id),0)=$5)
            ON CONFLICT(account_id) DO UPDATE SET payment_required_at=excluded.payment_required_at,
            payment_required_code=excluded.payment_required_code,cooldown_revision=supplier_health.cooldown_revision+1")
            .bind(code).bind(id).bind(auth_revision).bind(expected_observation).bind(expected_observation)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::Pools]);
        Ok(())
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
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_supplier(&mut tx, id).await?;
        sqlx::query("INSERT INTO supplier_health(account_id,cooldown_kind,cooldown_until,cooldown_auth_revision,cooldown_code,cooldown_observed_at,cooldown_revision)
            SELECT id,'quota_exhausted',$1,auth_revision,$2,floor(extract(epoch FROM clock_timestamp()))::bigint,1 FROM supplier_accounts a WHERE id=$3 AND auth_revision=$4
            AND ($5 IS NULL OR COALESCE((SELECT cooldown_revision FROM supplier_health WHERE account_id=a.id),0)=$6)
            ON CONFLICT(account_id) DO UPDATE SET
            cooldown_kind=excluded.cooldown_kind,
            cooldown_until=CASE WHEN supplier_health.cooldown_kind='quota_exhausted' AND supplier_health.cooldown_auth_revision=excluded.cooldown_auth_revision THEN GREATEST(COALESCE(supplier_health.cooldown_until,0),excluded.cooldown_until) ELSE excluded.cooldown_until END,
            cooldown_auth_revision=excluded.cooldown_auth_revision,cooldown_code=excluded.cooldown_code,cooldown_observed_at=excluded.cooldown_observed_at,cooldown_revision=supplier_health.cooldown_revision+1")
            .bind(until).bind(code).bind(id).bind(auth_revision).bind(expected).bind(expected).execute(&mut *tx).await?;
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::Pools]);
        Ok(())
    }

    /// A model-scoped exhaustion observed with the credential revision of that attempt.
    /// The account's models whose descriptor names the same `quota_pool` share it;
    /// other models stay schedulable, and a newer login discards the record.
    pub async fn exhaust_supplier_model_quota(
        &self,
        id: &str,
        auth_revision: i64,
        model: &str,
        until: i64,
        code: &str,
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_supplier(&mut tx, id).await?;
        sqlx::query(&format!(
            "INSERT INTO supplier_model_cooldowns(account_id,model,cooldown_until,cooldown_auth_revision,cooldown_code,cooldown_observed_at)
             SELECT a.id,p.model,$2,a.auth_revision,$3,floor(extract(epoch FROM clock_timestamp()))::bigint
             FROM supplier_accounts a CROSS JOIN ({}) p WHERE a.id=$4 AND a.auth_revision=$5
             ON CONFLICT(account_id,model) DO UPDATE SET
             cooldown_until=CASE WHEN supplier_model_cooldowns.cooldown_auth_revision=excluded.cooldown_auth_revision THEN GREATEST(supplier_model_cooldowns.cooldown_until,excluded.cooldown_until) ELSE excluded.cooldown_until END,
             cooldown_auth_revision=excluded.cooldown_auth_revision,cooldown_code=excluded.cooldown_code,cooldown_observed_at=excluded.cooldown_observed_at",
            pool_models("$1", "$4")
        ))
        .bind(model)
        .bind(until)
        .bind(code)
        .bind(id)
        .bind(auth_revision)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::Pools]);
        Ok(())
    }

    /// An official read reported the model available again before its recorded reset;
    /// the models sharing its quota pool recover with it.
    pub async fn recover_supplier_model_quota(&self, id: &str, model: &str) -> Result<()> {
        let changed = sqlx::query(&format!(
            "DELETE FROM supplier_model_cooldowns WHERE account_id=$1 AND model IN ({})",
            pool_models("$2", "$1")
        ))
        .bind(id)
        .bind(model)
        .execute(self.pool())
        .await?
        .rows_affected();
        if changed > 0 {
            self.runtime
                .snapshots
                .invalidate(&[crate::snapshots::Domain::Pools]);
        }
        Ok(())
    }

    /// Model cooldowns still in force for the account's current credentials.
    pub async fn supplier_model_cooldowns(&self, id: &str) -> Result<Vec<SupplierModelCooldown>> {
        Ok(sqlx::query_as(
            "SELECT c.model,c.cooldown_until,c.cooldown_code FROM supplier_model_cooldowns c
             JOIN supplier_accounts a ON a.id=c.account_id
             WHERE c.account_id=$1 AND c.cooldown_auth_revision=a.auth_revision
             AND c.cooldown_until>floor(extract(epoch FROM clock_timestamp()))::bigint
             ORDER BY c.model",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await?)
    }

    /// Administrator-requested retry, independent of provider reachability.
    /// Billing and quota state are cleared; credential rejection and manual
    /// enablement remain independent. In-flight checks lose their revision.
    pub async fn reset_supplier_availability(&self, id: &str) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_supplier(&mut tx, id).await?;
        sqlx::query(
            "INSERT INTO supplier_health(account_id,cooldown_revision)
             SELECT id,1 FROM supplier_accounts WHERE id=$1
             ON CONFLICT(account_id) DO UPDATE SET cooldown_kind=NULL,cooldown_until=NULL,
             cooldown_auth_revision=NULL,cooldown_code=NULL,cooldown_observed_at=NULL,
             payment_required_at=NULL,payment_required_code=NULL,
             cooldown_revision=supplier_health.cooldown_revision+1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM supplier_model_cooldowns WHERE account_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::Pools]);
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
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_supplier(&mut tx, id).await?;
        let changed = sqlx::query(
            "UPDATE supplier_health SET cooldown_kind=NULL,cooldown_until=NULL,
             cooldown_auth_revision=NULL,cooldown_code=NULL,cooldown_observed_at=NULL,
             cooldown_revision=cooldown_revision+1
             WHERE account_id=$1 AND cooldown_kind='quota_exhausted'
             AND cooldown_auth_revision=$2 AND cooldown_revision=$3
             AND EXISTS(SELECT 1 FROM supplier_accounts WHERE id=$4 AND auth_revision=$5)",
        )
        .bind(id)
        .bind(auth_revision)
        .bind(cooldown_revision)
        .bind(id)
        .bind(auth_revision)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        tx.commit().await?;
        if changed {
            self.runtime
                .snapshots
                .invalidate(&[crate::snapshots::Domain::Pools]);
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
             WHERE a.provider_id=$1 AND a.status='active' AND COALESCE(t.access_token,'')!=''
             AND h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision
             AND h.cooldown_until>floor(extract(epoch FROM clock_timestamp()))::bigint
             AND h.payment_required_at IS NULL
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
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_supplier(&mut tx, id).await?;
        sqlx::query(
            "INSERT INTO supplier_health(account_id,error_message,error_at,revision,rejected_auth_revision)
             SELECT id,'供应账户授权已失效，请重新授权',to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'),1,auth_revision
             FROM supplier_accounts WHERE id=$1 AND auth_revision=$2
             ON CONFLICT(account_id) DO UPDATE SET error_message=excluded.error_message,
             error_at=excluded.error_at,revision=supplier_health.revision+1,rejected_auth_revision=excluded.rejected_auth_revision",
        ).bind(id).bind(auth_revision).execute(&mut *tx).await?;
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::Pools]);
        Ok(())
    }
}

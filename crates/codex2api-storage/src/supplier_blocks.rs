//! Supplier availability as blocks judged by the supplier channel. A block avoids an
//! account, or one model of it, for this request only (never persisted), until a time,
//! or permanently. Blocks end by expiry, successful reauthorization, administrator
//! reset or a channel's own recovery observation; the executor never reads a
//! channel's error vocabulary, only these rows.
use crate::{Result, Storage, StorageError};
use serde::Serialize;
use sqlx::FromRow;

/// The models sharing a quota with model `model` of account `account` (SQL
/// placeholders): the model itself and every model of that account whose descriptor
/// names the same `quota_pool`, so one quota's models block and recover together.
fn pool_models(model: &str, account: &str) -> String {
    format!(
        "SELECT {model}::text AS model UNION SELECT m.model FROM supplier_models m
         WHERE m.account_id={account} AND m.descriptor_json::jsonb->>'quota_pool'=(
             SELECT s.descriptor_json::jsonb->>'quota_pool' FROM supplier_models s
             WHERE s.account_id={account} AND s.model={model})"
    )
}

const NOW: &str = "floor(extract(epoch FROM clock_timestamp()))::bigint";

/// Why a supplier is avoided. `Auth` and `Billing` are permanent until a new
/// authorization or an administrator reset; the others usually carry an expiry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Auth,
    Billing,
    RateLimit,
    Quota,
    Overload,
    Transport,
    ModelMissing,
    Penalty,
}
impl BlockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::Billing => "billing",
            Self::RateLimit => "rate_limit",
            Self::Quota => "quota",
            Self::Overload => "overload",
            Self::Transport => "transport",
            Self::ModelMissing => "model_missing",
            Self::Penalty => "penalty",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "auth" => Self::Auth,
            "billing" => Self::Billing,
            "rate_limit" => Self::RateLimit,
            "quota" => Self::Quota,
            "overload" => Self::Overload,
            "transport" => Self::Transport,
            "model_missing" => Self::ModelMissing,
            "penalty" => Self::Penalty,
            _ => return None,
        })
    }
    /// The administrator-visible status of an account carrying this block.
    pub fn display_status(self) -> &'static str {
        match self {
            Self::Auth => "error",
            Self::Billing => "payment_required",
            Self::RateLimit => "rate_limited",
            Self::Quota => "quota_exhausted",
            Self::Overload => "overloaded",
            Self::Transport | Self::ModelMissing | Self::Penalty => "blocked",
        }
    }
}

/// One avoidance to persist, observed on `auth_revision` of the account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRequest {
    /// `None` blocks the account; `Some(model)` blocks that model and the models
    /// sharing its quota pool.
    pub model: Option<String>,
    pub kind: BlockKind,
    /// Unix seconds the block ends, or `None` for a permanent block.
    pub until_at: Option<i64>,
    pub code: Option<String>,
    pub message: Option<String>,
}

/// An active block of a supplier account.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SupplierBlock {
    pub scope: String,
    pub model: String,
    pub kind: String,
    pub until_at: Option<i64>,
    pub code: Option<String>,
    pub message: Option<String>,
    pub observed_at: i64,
}
impl SupplierBlock {
    pub fn kind(&self) -> Option<BlockKind> {
        BlockKind::parse(&self.kind)
    }
}

/// The administrator's view of a supplier's availability: its active blocks and
/// the last authorization failure message.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SupplierHealth {
    pub error_message: Option<String>,
    pub error_at: Option<String>,
    pub revision: i64,
    /// Advances on every block write; recovery observations that began earlier
    /// cannot clear a newer failure.
    pub observation_revision: i64,
    pub blocks: Vec<SupplierBlock>,
}
impl SupplierHealth {
    pub fn account_block(&self) -> Option<&SupplierBlock> {
        self.blocks.iter().find(|block| block.scope == "account")
    }
    pub fn model_blocks(&self) -> impl Iterator<Item = &SupplierBlock> {
        self.blocks.iter().filter(|block| block.scope == "model")
    }
    fn account_kind(&self) -> Option<BlockKind> {
        self.account_block().and_then(SupplierBlock::kind)
    }
    pub fn authentication_invalid(&self) -> bool {
        self.account_kind() == Some(BlockKind::Auth)
    }
    pub fn payment_required(&self) -> bool {
        self.account_kind() == Some(BlockKind::Billing)
    }
    pub fn quota_exhausted(&self) -> bool {
        self.account_kind() == Some(BlockKind::Quota)
    }
    pub fn display_status(&self, enabled: bool, authorized: bool) -> &'static str {
        if !enabled {
            "disabled"
        } else if !authorized {
            "error"
        } else {
            self.account_kind()
                .map_or("active", BlockKind::display_status)
        }
    }
}

/// A successful explicit login starts a new authorization generation even if
/// the provider returns identical token bytes, and ends every block of the
/// account. It never changes manual enablement.
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
        "UPDATE supplier_health SET error_message=NULL,error_at=NULL,revision=revision+1,
         observation_revision=observation_revision+1 WHERE account_id=$1",
    )
    .bind(id)
    .execute(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM supplier_blocks WHERE account_id=$1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

impl Storage {
    pub async fn supplier_health(&self, id: &str) -> Result<SupplierHealth> {
        let (error_message, error_at, revision, observation_revision) = sqlx::query_as::<
            _,
            (Option<String>, Option<String>, i64, i64),
        >(
            "SELECT error_message,error_at,revision,observation_revision FROM supplier_health WHERE account_id=$1",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?
        .unwrap_or_default();
        let blocks = sqlx::query_as(&format!(
            "SELECT scope,model,kind,until_at,code,message,observed_at FROM supplier_blocks
             WHERE account_id=$1 AND (until_at IS NULL OR until_at>{NOW})
             ORDER BY scope,model"
        ))
        .bind(id)
        .fetch_all(self.pool())
        .await?;
        Ok(SupplierHealth {
            error_message,
            error_at,
            revision,
            observation_revision,
            blocks,
        })
    }

    /// Persist an avoidance observed with the credential revision of that attempt.
    /// A permanent block outranks a timed one and a later expiry outranks an earlier
    /// one; a model block also covers the models sharing its quota pool. Returns
    /// whether the account's current credentials were the observed ones.
    pub async fn block_supplier(
        &self,
        id: &str,
        auth_revision: i64,
        block: &BlockRequest,
    ) -> Result<bool> {
        self.block_supplier_fenced(id, auth_revision, block, None)
            .await
    }

    /// Like [`Storage::block_supplier`], applied only while no other observation
    /// advanced the account's `observation_revision` since `expected`.
    pub async fn block_supplier_fenced(
        &self,
        id: &str,
        auth_revision: i64,
        block: &BlockRequest,
        expected: Option<i64>,
    ) -> Result<bool> {
        if block.until_at.is_some_and(|until| until <= 0) {
            return Err(StorageError::Constraint(
                "block expiry must be positive".into(),
            ));
        }
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_supplier(&mut tx, id).await?;
        let current: Option<(i64, i64)> = sqlx::query_as(
            "SELECT a.auth_revision,COALESCE(h.observation_revision,0) FROM supplier_accounts a
             LEFT JOIN supplier_health h ON h.account_id=a.id WHERE a.id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((revision, observation)) = current else {
            return Err(StorageError::AccountNotFound(id.into()));
        };
        if revision != auth_revision || expected.is_some_and(|expected| expected != observation) {
            return Ok(false);
        }
        sqlx::query(&format!(
            "DELETE FROM supplier_blocks WHERE account_id=$1 AND until_at IS NOT NULL AND until_at<={NOW}"
        ))
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let models = match &block.model {
            Some(_) => format!(
                "SELECT 'model'::text AS scope,p.model FROM ({}) p",
                pool_models("$6", "$1")
            ),
            None => "SELECT 'account'::text AS scope,$6::text AS model".into(),
        };
        let model = block.model.clone().unwrap_or_default();
        sqlx::query(&format!(
            "INSERT INTO supplier_blocks(account_id,scope,model,kind,until_at,code,message,observed_at,auth_revision)
             SELECT $1,s.scope,s.model,$2,$3,$4,$5,{NOW},$7 FROM ({models}) s
             ON CONFLICT(account_id,scope,model) DO UPDATE SET
             kind=CASE WHEN supplier_blocks.until_at IS NULL AND excluded.until_at IS NOT NULL THEN supplier_blocks.kind ELSE excluded.kind END,
             until_at=CASE WHEN supplier_blocks.until_at IS NULL OR excluded.until_at IS NULL THEN NULL ELSE GREATEST(supplier_blocks.until_at,excluded.until_at) END,
             code=CASE WHEN supplier_blocks.until_at IS NULL AND excluded.until_at IS NOT NULL THEN supplier_blocks.code ELSE excluded.code END,
             message=CASE WHEN supplier_blocks.until_at IS NULL AND excluded.until_at IS NOT NULL THEN supplier_blocks.message ELSE excluded.message END,
             observed_at=excluded.observed_at,auth_revision=excluded.auth_revision"
        ))
        .bind(id)
        .bind(block.kind.as_str())
        .bind(block.until_at)
        .bind(&block.code)
        .bind(&block.message)
        .bind(&model)
        .bind(auth_revision)
        .execute(&mut *tx)
        .await?;
        let (message, at) = if block.kind == BlockKind::Auth {
            (
                Some(
                    block
                        .message
                        .clone()
                        .unwrap_or_else(|| "供应账户授权已失效，请重新授权".into()),
                ),
                true,
            )
        } else {
            (None, false)
        };
        sqlx::query(
            "INSERT INTO supplier_health(account_id,error_message,error_at,revision,observation_revision)
             VALUES($1,$2,CASE WHEN $3 THEN to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"') END,1,1)
             ON CONFLICT(account_id) DO UPDATE SET
             error_message=CASE WHEN $3 THEN excluded.error_message ELSE supplier_health.error_message END,
             error_at=CASE WHEN $3 THEN excluded.error_at ELSE supplier_health.error_at END,
             revision=CASE WHEN $3 THEN supplier_health.revision+1 ELSE supplier_health.revision END,
             observation_revision=supplier_health.observation_revision+1",
        )
        .bind(id)
        .bind(message)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::Pools]);
        Ok(true)
    }

    /// A channel's recovery observation ends the account's blocks of `kinds`, unless
    /// a newer failure or reauthorization happened since the observation began.
    pub async fn unblock_supplier(
        &self,
        id: &str,
        auth_revision: i64,
        observation_revision: i64,
        kinds: &[BlockKind],
    ) -> Result<bool> {
        let kinds: Vec<&str> = kinds.iter().map(|kind| kind.as_str()).collect();
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_supplier(&mut tx, id).await?;
        let changed = sqlx::query(
            "DELETE FROM supplier_blocks WHERE account_id=$1 AND kind=ANY($2) AND auth_revision=$3
             AND EXISTS(SELECT 1 FROM supplier_accounts a JOIN supplier_health h ON h.account_id=a.id
                        WHERE a.id=$1 AND a.auth_revision=$3 AND h.observation_revision=$4)",
        )
        .bind(id)
        .bind(&kinds)
        .bind(auth_revision)
        .bind(observation_revision)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed > 0 {
            sqlx::query(
                "UPDATE supplier_health SET observation_revision=observation_revision+1 WHERE account_id=$1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        if changed > 0 {
            self.runtime
                .snapshots
                .invalidate(&[crate::snapshots::Domain::Pools]);
        }
        Ok(changed > 0)
    }

    /// A recovery observation for one model: its block and the blocks of the models
    /// sharing its quota pool end together.
    pub async fn unblock_supplier_model(
        &self,
        id: &str,
        auth_revision: i64,
        model: &str,
        observation_revision: i64,
    ) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_supplier(&mut tx, id).await?;
        let changed = sqlx::query(&format!(
            "DELETE FROM supplier_blocks WHERE account_id=$1 AND scope='model' AND model IN ({})
             AND auth_revision=$3 AND EXISTS(
                 SELECT 1 FROM supplier_accounts a JOIN supplier_health h ON h.account_id=a.id
                 WHERE a.id=$1 AND a.auth_revision=$3 AND h.observation_revision=$4)",
            pool_models("$2", "$1")
        ))
        .bind(id)
        .bind(model)
        .bind(auth_revision)
        .bind(observation_revision)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        if changed > 0 {
            self.runtime
                .snapshots
                .invalidate(&[crate::snapshots::Domain::Pools]);
        }
        Ok(changed > 0)
    }

    /// Active model blocks of the account.
    pub async fn supplier_model_blocks(&self, id: &str) -> Result<Vec<SupplierBlock>> {
        Ok(sqlx::query_as(&format!(
            "SELECT scope,model,kind,until_at,code,message,observed_at FROM supplier_blocks
             WHERE account_id=$1 AND scope='model' AND (until_at IS NULL OR until_at>{NOW})
             ORDER BY model"
        ))
        .bind(id)
        .fetch_all(self.pool())
        .await?)
    }

    /// Administrator-requested retry, independent of provider reachability: every
    /// block except a credential rejection ends. Credential rejection and manual
    /// enablement remain independent; in-flight observations lose their fence.
    pub async fn reset_supplier_availability(&self, id: &str) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_supplier(&mut tx, id).await?;
        sqlx::query("DELETE FROM supplier_blocks WHERE account_id=$1 AND kind!='auth'")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO supplier_health(account_id,observation_revision)
             SELECT id,1 FROM supplier_accounts WHERE id=$1
             ON CONFLICT(account_id) DO UPDATE SET observation_revision=supplier_health.observation_revision+1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.runtime
            .snapshots
            .invalidate(&[crate::snapshots::Domain::Pools]);
        Ok(())
    }

    /// Accounts whose quota block is still active and might have recovered early:
    /// enabled, authorized, and not credential- or billing-rejected.
    pub async fn supplier_quota_probe_candidates(&self, provider: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(&format!(
            "SELECT a.id FROM supplier_accounts a
             JOIN supplier_blocks b ON b.account_id=a.id AND b.scope='account' AND b.kind='quota'
             JOIN supplier_tokens t ON t.account_id=a.id
             WHERE a.provider_id=$1 AND a.status='active' AND COALESCE(t.access_token,'')!=''
             AND b.until_at>{NOW} AND b.auth_revision=a.auth_revision
             ORDER BY b.observed_at,a.id"
        ))
        .bind(provider)
        .fetch_all(self.pool())
        .await?)
    }

    /// The caller observed an upstream credential rejection on this revision. Late
    /// failures cannot reject credentials that have since been refreshed or replaced.
    pub async fn reject_supplier_auth(&self, id: &str, auth_revision: i64) -> Result<()> {
        self.block_supplier(
            id,
            auth_revision,
            &BlockRequest {
                model: None,
                kind: BlockKind::Auth,
                until_at: None,
                code: None,
                message: None,
            },
        )
        .await?;
        Ok(())
    }
}

use crate::{Result, Storage, StorageError};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TurnStateSettings {
    pub enabled: bool,
    pub models: Vec<String>,
    pub ttl: i64,
    pub renew: i64,
    pub cooldown: i64,
}
impl Default for TurnStateSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            models: vec!["gpt-6-astra".into()],
            ttl: 3600,
            renew: 600,
            cooldown: 300,
        }
    }
}
impl TurnStateSettings {
    pub fn validate(&self) -> Result<()> {
        if self.models.is_empty()
            || self.models.len() > 16
            || self.models.iter().any(|m| {
                m.is_empty()
                    || m.len() > 128
                    || !m
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            })
            || !(120..=3600).contains(&self.ttl)
            || !(30..self.ttl).contains(&self.renew)
            || !(30..=3600).contains(&self.cooldown)
        {
            return Err(StorageError::InvalidTurnStateSettings);
        }
        Ok(())
    }
}

// Intentionally neither Debug nor Serialize: token must never appear in status responses/logs.
#[derive(FromRow)]
pub struct TurnStateCache {
    pub account_id: String,
    pub model: String,
    pub owner: String,
    pub revision: String,
    pub token: Option<String>,
    pub issued_at: i64,
    pub expires_at: i64,
    pub refresh_at: i64,
    pub next_probe_at: i64,
    pub lease_until: i64,
    pub lease: String,
    pub last_probe_at: i64,
    pub probe_status: i64,
    pub probe_result: String,
    pub injections: i64,
    pub client_states: i64,
    pub strikes: i64,
}

impl Storage {
    pub async fn turn_state_settings(&self, account: &str) -> Result<(TurnStateSettings, String)> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT config, revision FROM turn_state_settings WHERE account_id = ?")
                .bind(account)
                .fetch_optional(self.pool())
                .await?;
        match row {
            Some((config, revision)) => Ok((serde_json::from_str(&config)?, revision)),
            None => Ok((TurnStateSettings::default(), String::new())),
        }
    }
    pub async fn save_turn_state_settings(
        &self,
        account: &str,
        settings: &TurnStateSettings,
    ) -> Result<()> {
        settings.validate()?;
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO turn_state_settings(account_id, revision, config) VALUES (?, ?, ?) ON CONFLICT(account_id) DO UPDATE SET revision=excluded.revision, config=excluded.config")
            .bind(account).bind(uuid::Uuid::new_v4().to_string()).bind(serde_json::to_string(settings)?)
            .execute(&mut *tx).await?;
        sqlx::query("DELETE FROM turn_state_cache WHERE account_id=?")
            .bind(account)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn clear_turn_state(&self, account: &str) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("UPDATE turn_state_settings SET revision=? WHERE account_id=?")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(account)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM turn_state_cache WHERE account_id=?")
            .bind(account)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn turn_state_entries(&self, account: &str) -> Result<Vec<TurnStateCache>> {
        Ok(
            sqlx::query_as("SELECT * FROM turn_state_cache WHERE account_id=? ORDER BY model")
                .bind(account)
                .fetch_all(self.pool())
                .await?,
        )
    }
    pub async fn turn_state_entry(
        &self,
        account: &str,
        model: &str,
        owner: &str,
        revision: &str,
    ) -> Result<Option<TurnStateCache>> {
        Ok(sqlx::query_as("SELECT * FROM turn_state_cache WHERE account_id=? AND model=? AND owner=? AND revision=?")
            .bind(account).bind(model).bind(owner).bind(revision).fetch_optional(self.pool()).await?)
    }
    pub async fn ensure_turn_state_entry(
        &self,
        account: &str,
        model: &str,
        owner: &str,
        revision: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO turn_state_cache(account_id, model, owner, revision) SELECT s.account_id, ?, ?, s.revision FROM turn_state_settings s JOIN accounts a ON a.id=s.account_id WHERE s.account_id=? AND s.revision=? AND a.chatgpt_account_id=? AND a.status='active' ON CONFLICT(account_id,model) DO NOTHING")
            .bind(model).bind(owner).bind(account).bind(revision).bind(owner).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn claim_turn_state_probe(
        &self,
        entry: &TurnStateCache,
        now: i64,
        cooldown: i64,
    ) -> Result<Option<String>> {
        let lease = uuid::Uuid::new_v4().to_string();
        let changed = sqlx::query("UPDATE turn_state_cache SET lease=?, lease_until=?, next_probe_at=? WHERE account_id=? AND model=? AND owner=? AND revision=? AND lease_until<=? AND next_probe_at<=? AND (refresh_at<=? OR expires_at<=?)")
            .bind(&lease).bind(now+25).bind(now+cooldown).bind(&entry.account_id).bind(&entry.model).bind(&entry.owner).bind(&entry.revision)
            .bind(now).bind(now).bind(now).bind(now).execute(self.pool()).await?.rows_affected();
        Ok((changed == 1).then_some(lease))
    }
    pub async fn finish_turn_state_probe(
        &self,
        entry: &TurnStateCache,
        lease: &str,
        result: TurnStateProbeResult<'_>,
    ) -> Result<()> {
        // Lease identity prevents a cleared/reconfigured/reauthorized request from resurrecting state.
        sqlx::query("UPDATE turn_state_cache SET token=CASE WHEN ? IS NULL THEN token ELSE ? END, issued_at=CASE WHEN ? IS NULL THEN issued_at ELSE ? END, expires_at=CASE WHEN ? IS NULL THEN expires_at ELSE ? END, refresh_at=CASE WHEN ? IS NULL THEN refresh_at ELSE ? END, strikes=CASE WHEN ? IS NULL THEN strikes ELSE 0 END, lease_until=0, lease='', last_probe_at=?, probe_status=?, probe_result=?, next_probe_at=? WHERE account_id=? AND model=? AND revision=? AND owner=? AND lease=?")
            .bind(result.token).bind(result.token).bind(result.token).bind(result.issued_at)
            .bind(result.token).bind(result.expires_at).bind(result.token).bind(result.refresh_at).bind(result.token)
            .bind(result.now).bind(result.status).bind(result.result).bind(result.next_probe_at)
            .bind(&entry.account_id).bind(&entry.model).bind(&entry.revision).bind(&entry.owner).bind(lease).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn record_turn_state_use(
        &self,
        entry: &TurnStateCache,
        injected: bool,
    ) -> Result<()> {
        sqlx::query("UPDATE turn_state_cache SET injections=injections+?, client_states=client_states+? WHERE account_id=? AND model=? AND owner=? AND revision=?")
            .bind(i64::from(injected)).bind(i64::from(!injected)).bind(&entry.account_id).bind(&entry.model).bind(&entry.owner).bind(&entry.revision).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn observe_turn_state(&self, entry: &TurnStateCache, valid: bool) -> Result<()> {
        sqlx::query("UPDATE turn_state_cache SET refresh_at=CASE WHEN ?=0 AND strikes>=1 THEN 0 ELSE refresh_at END, strikes=CASE WHEN ? THEN 0 ELSE strikes+1 END WHERE account_id=? AND model=? AND owner=? AND revision=? AND token=?")
            .bind(valid).bind(valid).bind(&entry.account_id).bind(&entry.model).bind(&entry.owner).bind(&entry.revision).bind(&entry.token).execute(self.pool()).await?;
        Ok(())
    }
}

pub struct TurnStateProbeResult<'a> {
    pub token: Option<&'a str>,
    pub issued_at: i64,
    pub expires_at: i64,
    pub refresh_at: i64,
    pub now: i64,
    pub status: i64,
    pub result: &'a str,
    pub next_probe_at: i64,
}

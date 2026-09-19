use crate::{Result, Storage, StorageError};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TurnStateSettings {
    pub enabled: bool,
    pub models: Vec<String>,
    pub ttl: i64,
}
impl Default for TurnStateSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            models: vec!["gpt-6-astra".into()],
            ttl: 3600,
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
        {
            return Err(StorageError::InvalidTurnStateSettings);
        }
        Ok(())
    }
}

// Never Debug/Serialize: the opaque token must not enter logs or admin responses.
#[derive(FromRow)]
pub struct TurnStateCache {
    pub account_id: String,
    pub model: String,
    pub owner: String,
    pub revision: String,
    pub token: Option<String>,
    pub source: String,
    pub captured_at: i64,
    pub issued_at: i64,
    pub expires_at: i64,
    pub injections: i64,
    pub request_count: i64,
    pub response_count: i64,
    pub request_captures: i64,
    pub response_captures: i64,
    pub last_request_at: i64,
    pub request_result: String,
    pub request_length: i64,
    pub request_blocks: i64,
    pub last_response_at: i64,
    pub response_status: i64,
    pub response_result: String,
    pub response_length: i64,
    pub response_blocks: i64,
}

pub struct TurnStateObservation<'a> {
    pub from_client: bool,
    pub token: Option<&'a str>,
    pub issued_at: i64,
    pub now: i64,
    pub status: i64,
    pub result: &'static str,
    pub length: i64,
    pub blocks: i64,
    pub injected: bool,
}

impl Storage {
    pub async fn turn_state_settings(&self, account: &str) -> Result<(TurnStateSettings, String)> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT config, revision FROM turn_state_settings WHERE account_id=?")
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
        sqlx::query("INSERT INTO turn_state_settings(account_id,revision,config) VALUES (?,?,?) ON CONFLICT(account_id) DO UPDATE SET revision=excluded.revision,config=excluded.config")
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
        sqlx::query("INSERT INTO turn_state_cache(account_id,model,owner,revision) SELECT s.account_id,?,?,s.revision FROM turn_state_settings s JOIN accounts a ON a.id=s.account_id WHERE s.account_id=? AND s.revision=? AND a.chatgpt_account_id=? AND a.status='active' ON CONFLICT(account_id,model) DO NOTHING")
            .bind(model).bind(owner).bind(account).bind(revision).bind(owner).execute(self.pool()).await?;
        Ok(())
    }

    /// Commit diagnostics and a natural capture together, scoped to the configuration generation.
    pub async fn record_turn_state_observation(
        &self,
        entry: &TurnStateCache,
        settings: &TurnStateSettings,
        observation: TurnStateObservation<'_>,
    ) -> Result<()> {
        let o = observation;
        let mut tx = self.pool().begin().await?;
        let mut result = o.result;
        let mut captured = false;
        if let Some(token) = o.token {
            captured = sqlx::query("UPDATE turn_state_cache SET token=?,issued_at=?,expires_at=?,source=?,captured_at=? WHERE account_id=? AND model=? AND owner=? AND revision=? AND (token IS NULL OR issued_at<=?)")
                .bind(token).bind(o.issued_at).bind(o.issued_at+settings.ttl-30)
                .bind(if o.from_client { "client" } else { "response" }).bind(o.now)
                .bind(&entry.account_id).bind(&entry.model).bind(&entry.owner).bind(&entry.revision).bind(o.issued_at)
                .execute(&mut *tx).await?.rows_affected() == 1;
            if !captured {
                result = "older_state";
            }
        }
        if o.from_client {
            sqlx::query("UPDATE turn_state_cache SET request_count=request_count+1,last_request_at=?,request_result=?,request_length=?,request_blocks=?,request_captures=request_captures+?,injections=injections+? WHERE account_id=? AND model=? AND owner=? AND revision=?")
                .bind(o.now).bind(result).bind(o.length).bind(o.blocks).bind(i64::from(captured)).bind(i64::from(o.injected))
                .bind(&entry.account_id).bind(&entry.model).bind(&entry.owner).bind(&entry.revision)
                .execute(&mut *tx).await?;
        } else {
            sqlx::query("UPDATE turn_state_cache SET response_count=response_count+1,last_response_at=?,response_result=?,response_length=?,response_blocks=?,response_captures=response_captures+?,response_status=? WHERE account_id=? AND model=? AND owner=? AND revision=?")
                .bind(o.now).bind(result).bind(o.length).bind(o.blocks).bind(i64::from(captured)).bind(o.status)
                .bind(&entry.account_id).bind(&entry.model).bind(&entry.owner).bind(&entry.revision)
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

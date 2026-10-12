use crate::{QuotaSnapshot, Result, Storage, SupplierAccount, SupplierTokens};
use chrono::{DateTime, Utc};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

/// Credentials and their generation read together; never expose this record in an API.
#[derive(sqlx::FromRow)]
pub struct SupplierAuthSnapshot {
    #[sqlx(flatten)]
    pub tokens: SupplierTokens,
    pub auth_revision: i64,
    pub provider_id: String,
    pub chatgpt_account_id: Option<String>,
    #[sqlx(skip)]
    pub cache_generation: u64,
    #[sqlx(skip)]
    decoded: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl SupplierAuthSnapshot {
    /// A provider decodes its own credentials once per immutable snapshot.
    pub fn decode<T: Send + Sync + 'static, E>(
        &self,
        decode: impl FnOnce() -> std::result::Result<T, E>,
    ) -> std::result::Result<Arc<T>, E> {
        let mut values = self.decoded.lock().unwrap();
        if let Some(value) = values.get(&TypeId::of::<T>()) {
            return Ok(value
                .clone()
                .downcast()
                .unwrap_or_else(|_| unreachable!("credential type key")));
        }
        let value = Arc::new(decode()?);
        values.insert(TypeId::of::<T>(), value.clone());
        Ok(value)
    }
}

impl Storage {
    pub async fn supplier_account_snapshot(&self, id: &str) -> Result<Arc<SupplierAccount>> {
        self.runtime
            .snapshot(format!("supplier-account:{id}"), async {
                Ok((
                    Arc::new(self.require_account(id).await?),
                    Duration::from_secs(30),
                ))
            })
            .await
    }
    pub async fn update_supplier_profile(
        &self,
        id: &str,
        revision: i64,
        email: Option<&str>,
        plan: Option<&str>,
    ) -> Result<bool> {
        let changed = sqlx::query("UPDATE supplier_accounts SET email=COALESCE(?,email),plan_type=COALESCE(?,plan_type),updated_at=? WHERE id=? AND auth_revision=?")
            .bind(email.filter(|s|!s.trim().is_empty())).bind(plan.filter(|s|!s.trim().is_empty())).bind(Utc::now().to_rfc3339()).bind(id).bind(revision)
            .execute(self.pool()).await?.rows_affected()==1;
        if changed {
            self.runtime
                .invalidate_key(&format!("supplier-account:{id}"));
        }
        Ok(changed)
    }
    /// A late refresh must never overwrite a newer login or administrator revocation.
    pub async fn replace_supplier_tokens(
        &self,
        id: &str,
        revision: i64,
        tokens: SupplierTokens,
    ) -> Result<bool> {
        let updated = sqlx::query("UPDATE supplier_tokens SET auth_mode=?,id_token=?,access_token=?,refresh_token=?,last_refresh=?,raw_auth_json=?,updated_at=? WHERE account_id=? AND EXISTS(SELECT 1 FROM supplier_accounts WHERE id=? AND auth_revision=?)")
            .bind(tokens.auth_mode).bind(tokens.id_token).bind(tokens.access_token).bind(tokens.refresh_token)
            .bind(tokens.last_refresh).bind(tokens.raw_auth_json).bind(Utc::now().to_rfc3339()).bind(id).bind(id).bind(revision)
            .execute(self.pool()).await?.rows_affected()==1;
        if updated {
            self.invalidate_runtime_cache();
        }
        Ok(updated)
    }
    pub async fn supplier_auth_snapshot(
        &self,
        id: &str,
    ) -> Result<Option<Arc<SupplierAuthSnapshot>>> {
        self.runtime.snapshot(format!("supplier-auth:{id}"), async {
            let generation = self.runtime.epoch();
            let mut value: Option<SupplierAuthSnapshot> = sqlx::query_as("SELECT t.*, a.auth_revision, a.provider_id, a.chatgpt_account_id FROM supplier_tokens t JOIN supplier_accounts a ON a.id=t.account_id WHERE a.id=?")
                .bind(id).fetch_optional(self.pool()).await?;
            if let Some(value) = &mut value { value.cache_generation = generation; }
            Ok((value.map(Arc::new), Duration::from_secs(30)))
        }).await
    }

    pub async fn supplier_auth_revision(&self, id: &str) -> Result<Option<i64>> {
        self.runtime
            .snapshot(format!("supplier-auth-revision:{id}"), async {
                let value =
                    sqlx::query_scalar("SELECT auth_revision FROM supplier_accounts WHERE id=?")
                        .bind(id)
                        .fetch_optional(self.pool())
                        .await?;
                Ok((value, Duration::from_secs(30)))
            })
            .await
    }

    pub async fn supplier_routing_snapshot(
        &self,
        id: &str,
    ) -> Result<Option<Arc<(QuotaSnapshot, Option<i64>)>>> {
        self.runtime.snapshot(format!("supplier-routing:{id}"), async {
        let row: Option<(String, DateTime<Utc>, Option<i64>)> = sqlx::query_as(
            "SELECT response_json, observed_at, auth_revision FROM supplier_info_cache WHERE account_id=? AND section='details'",
        ).bind(id).fetch_optional(self.pool()).await?;
        let value = row.map(|(json, observed_at, revision)| {
            Ok::<_, crate::StorageError>(Arc::new((
                QuotaSnapshot {
                    value: serde_json::from_str(&json)?,
                    observed_at,
                },
                revision,
            )))
        })
        .transpose()?;
        Ok((value, Duration::from_secs(30)))
        }).await
    }

    /// A discovery made with superseded credentials must not replace the current snapshot.
    pub async fn store_supplier_routing_snapshot(
        &self,
        id: &str,
        revision: i64,
        snapshot: &QuotaSnapshot,
    ) -> Result<bool> {
        let changed = self
            .write_request(crate::request_writer::Write::Routing {
                id: id.into(),
                revision,
                json: serde_json::to_string(&snapshot.value)?,
                at: snapshot.observed_at,
            })
            .await?
            == 1;
        if changed {
            self.runtime
                .invalidate_key(&format!("supplier-routing:{id}"));
        }
        Ok(changed)
    }

    /// Compare-and-set only derived UA fields; preserve concurrent fingerprint edits.
    pub async fn align_supplier_user_agent(
        &self,
        account: &SupplierAccount,
        user_agent: &str,
        fingerprint: &str,
    ) -> Result<bool> {
        let changed = sqlx::query("UPDATE supplier_accounts SET user_agent=?,http_fingerprint_json=? WHERE id=? AND user_agent=? AND http_fingerprint_json=?")
            .bind(user_agent).bind(fingerprint).bind(&account.id).bind(&account.user_agent).bind(&account.http_fingerprint_json)
            .execute(self.pool()).await?.rows_affected() == 1;
        if changed {
            self.invalidate_runtime_cache();
        }
        Ok(changed)
    }
}

use crate::{
    NewSupplierAccount, Result, Storage, StorageError, SupplierModel, SupplierScheduling,
    SupplierTokens,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct SupplierTransferProxy {
    pub name: String,
    pub url: String,
}

/// Administrator-only portable supplier data. Never used by ordinary account APIs.
#[derive(Clone, Serialize, Deserialize)]
pub struct SupplierTransferAccount {
    #[serde(flatten)]
    pub account: NewSupplierAccount,
    pub tokens: Option<SupplierTokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<SupplierTransferProxy>,
    #[serde(default)]
    pub models: Vec<SupplierModel>,
    #[serde(default)]
    pub scheduling: SupplierScheduling,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Storage {
    pub async fn export_supplier(
        &self,
        id: &str,
        include_proxy: bool,
    ) -> Result<SupplierTransferAccount> {
        let a = self.require_account(id).await?;
        let proxy = if let Some(id) = a.proxy_id.as_deref().filter(|_| include_proxy) {
            let proxy = self.require_outbound_proxy(id).await?;
            Some(SupplierTransferProxy {
                name: proxy.name,
                url: proxy.url,
            })
        } else {
            None
        };
        Ok(SupplierTransferAccount {
            tokens: self.load_supplier_tokens(id).await?,
            models: self.supplier_models(id).await?,
            scheduling: self.supplier_scheduling(id).await?,
            tags: self
                .supplier_tag_labels(id)
                .await?
                .into_iter()
                .map(|(_, name)| name)
                .collect(),
            proxy,
            account: NewSupplierAccount {
                provider_id: a.provider_id,
                id: Some(a.id),
                status: a.status,
                display_name: a.display_name,
                chatgpt_account_id: a.chatgpt_account_id,
                chatgpt_user_id: a.chatgpt_user_id,
                email: a.email,
                plan_type: a.plan_type,
                installation_id: a.installation_id,
                originator: a.originator,
                user_agent: a.user_agent,
                os_type: a.os_type,
                os_version: a.os_version,
                arch: a.arch,
                home_dir: String::new(),
                http_fingerprint_json: a.http_fingerprint_json,
            },
        })
    }

    /// Insert each supplier with its credentials, models and proxy in one transaction.
    /// Existing identities are skipped: importing a backup cannot overwrite a newer login.
    pub async fn import_supplier(&self, item: &SupplierTransferAccount) -> Result<bool> {
        let a = &item.account;
        let scheduling = &item.scheduling;
        if !codex2api_core::supported_provider(&a.provider_id)
            || item.models.len() > 2048
            || item.models.iter().any(|m| {
                !codex2api_core::valid_model(&m.model)
                    || !["custom", "upstream"].contains(&m.source.as_str())
            })
            || item.tags.len() > 1000
            || item
                .tags
                .iter()
                .any(|tag| tag.trim().is_empty() || tag.chars().count() > 80)
            || scheduling.concurrency > 1_000_000
            || !(0..=1_000_000).contains(&scheduling.priority)
            || scheduling
                .load_factor
                .is_some_and(|n| n == 0 || n > 1_000_000)
        {
            return Err(StorageError::Constraint("供应账户数据无效".into()));
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM supplier_accounts WHERE id=? OR installation_id=?
             OR (provider_id=? AND chatgpt_account_id=? AND chatgpt_user_id=?))",
        )
        .bind(&a.id)
        .bind(&a.installation_id)
        .bind(&a.provider_id)
        .bind(&a.chatgpt_account_id)
        .bind(&a.chatgpt_user_id)
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            return Ok(false);
        }
        let now = chrono::Utc::now().to_rfc3339();
        let id =
            a.id.clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let proxy_id = if let Some(proxy) = &item.proxy {
            let url = crate::parse_proxy_url(&proxy.url)?;
            if proxy.name.trim().is_empty() || proxy.name.chars().count() > 128 {
                return Err(StorageError::InvalidProxy);
            }
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT id FROM outbound_proxies WHERE url=? ORDER BY id LIMIT 1",
            )
            .bind(url.as_str())
            .fetch_optional(&mut *tx)
            .await?;
            Some(match existing {
                Some(id) => id,
                None => {
                    let id = uuid::Uuid::new_v4().to_string();
                    sqlx::query(
                        "INSERT INTO outbound_proxies(id,name,url,created_at) VALUES(?,?,?,?)",
                    )
                    .bind(&id)
                    .bind(proxy.name.trim())
                    .bind(url.as_str())
                    .bind(&now)
                    .execute(&mut *tx)
                    .await?;
                    id
                }
            })
        } else {
            None
        };
        sqlx::query("INSERT INTO supplier_accounts(id,provider_id,status,display_name,chatgpt_account_id,chatgpt_user_id,
            email,plan_type,installation_id,originator,user_agent,os_type,os_version,arch,home_dir,http_fingerprint_json,
            proxy_id,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,'',?,?,?,?)")
            .bind(&id).bind(&a.provider_id).bind(a.status.as_str()).bind(&a.display_name)
            .bind(&a.chatgpt_account_id).bind(&a.chatgpt_user_id).bind(&a.email).bind(&a.plan_type)
            .bind(&a.installation_id).bind(&a.originator).bind(&a.user_agent).bind(&a.os_type)
            .bind(&a.os_version).bind(&a.arch).bind(&a.http_fingerprint_json).bind(proxy_id).bind(&now).bind(&now)
            .execute(&mut *tx).await?;
        if let Some(t) = &item.tokens {
            sqlx::query("INSERT INTO supplier_tokens(account_id,auth_mode,id_token,access_token,refresh_token,last_refresh,raw_auth_json,updated_at)
                VALUES(?,?,?,?,?,?,?,?)")
                .bind(&id).bind(&t.auth_mode).bind(&t.id_token).bind(&t.access_token).bind(&t.refresh_token)
                .bind(&t.last_refresh).bind(&t.raw_auth_json).bind(&now).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO supplier_scheduling(account_id,concurrency,priority,load_factor,revision) VALUES(?,?,?,?,1)")
            .bind(&id).bind(scheduling.concurrency).bind(scheduling.priority).bind(scheduling.load_factor)
            .execute(&mut *tx).await?;
        for m in &item.models {
            sqlx::query("INSERT INTO supplier_models(account_id,model,source,observed_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING")
                .bind(&id).bind(&m.model).bind(&m.source).bind(&m.observed_at).execute(&mut *tx).await?;
        }
        for name in &item.tags {
            sqlx::query("INSERT INTO supplier_tags(id,provider_id,name) VALUES(?,?,?) ON CONFLICT(provider_id,name) DO NOTHING")
                .bind(uuid::Uuid::new_v4().to_string()).bind(&a.provider_id).bind(name.trim()).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO supplier_tag_members(tag_id,account_id) SELECT id,? FROM supplier_tags WHERE provider_id=? AND name=? ON CONFLICT DO NOTHING")
                .bind(&id).bind(&a.provider_id).bind(name.trim()).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        self.invalidate_runtime_cache();
        Ok(true)
    }
}

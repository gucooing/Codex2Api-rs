use crate::{Result, Storage, StorageError};
use std::{sync::Arc, time::Duration};

/// Frozen transport fields only. Authorization and availability are checked live.
#[derive(Clone, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct SupplierTransportSnapshot {
    pub id: String,
    pub provider_id: String,
    pub installation_id: String,
    pub originator: String,
    pub user_agent: String,
    pub os_type: String,
    pub os_version: String,
    pub arch: String,
    pub http_fingerprint_json: String,
    pub proxy_url: Option<String>,
    #[sqlx(skip)]
    pub cache_generation: u64,
}

impl Storage {
    pub async fn supplier_transport_snapshot(
        &self,
        id: &str,
    ) -> Result<Arc<SupplierTransportSnapshot>> {
        let key = format!("supplier-transport:{id}");
        self.runtime
            .snapshots
            .read(key.clone(), async {
                let mut value: SupplierTransportSnapshot = sqlx::query_as(
                    "SELECT a.id,a.provider_id,a.installation_id,a.originator,a.user_agent,
             a.os_type,a.os_version,a.arch,a.http_fingerprint_json,p.url AS proxy_url
             FROM supplier_accounts a LEFT JOIN outbound_proxies p ON p.id=a.proxy_id WHERE a.id=$1",
                )
                .bind(id)
                .fetch_optional(self.pool())
                .await?
                .ok_or_else(|| StorageError::AccountNotFound(id.into()))?;
                value.cache_generation = self.runtime.snapshots.version(&key);
                let value = Arc::new(value);
                Ok((value, Duration::from_secs(30)))
            })
            .await
    }
}

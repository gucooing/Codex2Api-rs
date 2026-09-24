use crate::{Result, Storage};
use serde_json::Value;

pub struct VirtualClientState {
    pub value: Value,
    pub revision: i64,
    pub write_origin: String,
    pub updated_at_ms: i64,
}

impl Storage {
    pub async fn virtual_client_state(
        &self,
        owner: &str,
        key: &str,
    ) -> Result<Option<VirtualClientState>> {
        let row:Option<(String,i64,String,i64)>=sqlx::query_as("SELECT value_json,revision,write_origin,updated_at_ms FROM virtual_client_state WHERE virtual_account_id=? AND state_key=?")
            .bind(owner).bind(key).fetch_optional(self.pool()).await?;
        row.map(|(value, revision, write_origin, updated_at_ms)| {
            Ok(VirtualClientState {
                value: serde_json::from_str(&value)?,
                revision,
                write_origin,
                updated_at_ms,
            })
        })
        .transpose()
    }

    /// One atomic revision check; state stays attached to the virtual account through rebinding.
    pub async fn save_virtual_client_state(
        &self,
        owner: &str,
        key: &str,
        value: &Value,
        expected: Option<i64>,
    ) -> Result<Option<i64>> {
        if !crate::client_writable(key) {
            return Err(crate::StorageError::Constraint(
                "客户端无权修改服务配置".into(),
            ));
        }
        if matches!(key, "user_settings" | "notifications") {
            let previous = self.virtual_config(owner, key).await?;
            crate::validate_client_change(key, &previous.value, value)
                .map_err(crate::StorageError::Constraint)?;
        }
        if crate::plan_owned_config(key) {
            return Err(crate::StorageError::Constraint(
                "套餐配置由套餐管理统一维护".into(),
            ));
        }
        Ok(sqlx::query_scalar("INSERT INTO virtual_client_state(virtual_account_id,state_key,value_json,revision,write_origin,updated_at_ms)
            SELECT id,?,?,1,'client',? FROM virtual_accounts WHERE id=? AND enabled=1 AND
              (? IS NULL OR ?=0 OR EXISTS(SELECT 1 FROM virtual_client_state WHERE virtual_account_id=? AND state_key=?))
            ON CONFLICT(virtual_account_id,state_key) DO UPDATE SET value_json=excluded.value_json,revision=virtual_client_state.revision+1,write_origin='client',updated_at_ms=excluded.updated_at_ms
            WHERE (? IS NULL OR virtual_client_state.revision=?) RETURNING revision")
            .bind(key).bind(serde_json::to_string(value)?).bind(chrono::Utc::now().timestamp_millis()).bind(owner).bind(expected).bind(expected).bind(owner).bind(key).bind(expected).bind(expected)
            .fetch_optional(self.pool()).await?)
    }
}

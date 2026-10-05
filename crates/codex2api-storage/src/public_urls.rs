use crate::{Result, Storage, StorageError};
use serde::{Deserialize, Serialize};

/// Browser-facing origins, independent of the process's listening addresses.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublicUrlSettings {
    pub api_url: String,
    pub user_url: String,
    pub admin_url: String,
    pub revision: i64,
}

pub fn normalize_public_origin(value: &str) -> Result<String> {
    let value = value.trim();
    let invalid = || {
        StorageError::InvalidAdminUpdate(
            "访问地址须为完整的 http:// 或 https:// 域名和可选端口，不含路径、账号、查询参数或片段",
        )
    };
    if value.len() > 2048
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
        || value.contains('\\')
        || !value.contains("://")
    {
        return Err(invalid());
    }
    let url = url::Url::parse(value).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(url.origin().ascii_serialization())
}

impl Storage {
    pub async fn public_url_settings(&self) -> Result<Option<PublicUrlSettings>> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM meta WHERE key = 'public_url_settings'")
                .fetch_optional(self.pool())
                .await?;
        value
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    /// Atomically replace all three origins, rejecting stale administrator forms.
    pub async fn save_public_url_settings(
        &self,
        settings: &PublicUrlSettings,
    ) -> Result<Option<PublicUrlSettings>> {
        if !(0..i64::MAX).contains(&settings.revision) {
            return Err(StorageError::InvalidAdminUpdate("访问地址版本无效"));
        }
        let saved = PublicUrlSettings {
            api_url: normalize_public_origin(&settings.api_url)?,
            user_url: normalize_public_origin(&settings.user_url)?,
            admin_url: normalize_public_origin(&settings.admin_url)?,
            revision: settings.revision + 1,
        };
        let value = serde_json::to_string(&saved)?;
        let changed = if settings.revision == 0 {
            sqlx::query("INSERT INTO meta (key, value) VALUES ('public_url_settings', ?) ON CONFLICT(key) DO NOTHING")
                .bind(value).execute(self.pool()).await?
        } else {
            sqlx::query("UPDATE meta SET value = ? WHERE key = 'public_url_settings' AND json_extract(value, '$.revision') = ?")
                .bind(value).bind(settings.revision).execute(self.pool()).await?
        };
        Ok((changed.rows_affected() == 1).then_some(saved))
    }
}

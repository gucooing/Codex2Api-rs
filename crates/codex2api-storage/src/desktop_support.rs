use crate::{Result, Storage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DesktopSupportSettings {
    pub proxy_id: Option<String>,
    pub resource_cache_minutes: u32,
}
impl Default for DesktopSupportSettings {
    fn default() -> Self {
        Self {
            proxy_id: None,
            resource_cache_minutes: 60,
        }
    }
}
pub struct DesktopResource {
    pub content: Vec<u8>,
    pub headers: Value,
    pub fetched_at_ms: i64,
}
impl Storage {
    pub async fn desktop_support_settings(&self) -> Result<DesktopSupportSettings> {
        let existing: Option<String> =
            sqlx::query_scalar("SELECT value FROM meta WHERE key='desktop_support'")
                .fetch_optional(self.pool())
                .await?;
        if let Some(raw) = existing {
            return Ok(serde_json::from_str(&raw)?);
        }
        let default = serde_json::to_string(&DesktopSupportSettings::default())?;
        sqlx::query("INSERT OR IGNORE INTO meta(key,value) VALUES('desktop_support',?)")
            .bind(default)
            .execute(self.pool())
            .await?;
        let raw: String = sqlx::query_scalar("SELECT value FROM meta WHERE key='desktop_support'")
            .fetch_one(self.pool())
            .await?;
        Ok(serde_json::from_str(&raw)?)
    }
    pub async fn save_desktop_support_settings(
        &self,
        value: &DesktopSupportSettings,
    ) -> Result<()> {
        sqlx::query("INSERT INTO meta(key,value) VALUES('desktop_support',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value").bind(serde_json::to_string(value)?).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn desktop_resource(&self, path: &str) -> Result<Option<DesktopResource>> {
        let row: Option<(Vec<u8>, String, i64)> = sqlx::query_as(
            "SELECT content,headers_json,fetched_at_ms FROM desktop_public_resources WHERE path=?",
        )
        .bind(path)
        .fetch_optional(self.pool())
        .await?;
        row.map(|(content, raw, fetched_at_ms)| {
            Ok(DesktopResource {
                content,
                headers: serde_json::from_str(&raw)?,
                fetched_at_ms,
            })
        })
        .transpose()
    }
    pub async fn save_desktop_resource(
        &self,
        path: &str,
        content: &[u8],
        headers: &Value,
    ) -> Result<()> {
        sqlx::query("INSERT INTO desktop_public_resources(path,content,headers_json,fetched_at_ms) VALUES(?,?,?,?) ON CONFLICT(path) DO UPDATE SET content=excluded.content,headers_json=excluded.headers_json,fetched_at_ms=excluded.fetched_at_ms")
            .bind(path).bind(content).bind(headers.to_string()).bind(chrono::Utc::now().timestamp_millis()).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn desktop_resources(&self) -> Result<Vec<Value>> {
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT path,length(content),fetched_at_ms FROM desktop_public_resources ORDER BY path",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(path, bytes, time)| json!({"path":path,"bytes":bytes,"fetched_at_ms":time}))
            .collect())
    }
}

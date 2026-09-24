use crate::{Result, Storage};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DesktopSupportSettings {
    pub proxy_id: Option<String>,
    pub collect_diagnostics: bool,
    pub resource_cache_minutes: u32,
}
impl Default for DesktopSupportSettings {
    fn default() -> Self {
        Self {
            proxy_id: None,
            collect_diagnostics: true,
            resource_cache_minutes: 60,
        }
    }
}
pub struct DesktopResource {
    pub content: Vec<u8>,
    pub headers: Value,
    pub fetched_at_ms: i64,
}
#[derive(sqlx::FromRow)]
struct DiagnosticRow {
    id: i64,
    source: String,
    virtual_account_id: Option<String>,
    record_count: i64,
    summaries_json: String,
    attempts: i64,
    first_seen_at_ms: i64,
    last_seen_at_ms: i64,
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
    pub async fn record_desktop_diagnostic(
        &self,
        hash: &str,
        source: &str,
        owner: Option<&str>,
        count: usize,
        summaries: &Value,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("INSERT INTO desktop_diagnostics(batch_hash,source,virtual_account_id,record_count,summaries_json,first_seen_at_ms,last_seen_at_ms) VALUES(?,?,?,?,?,?,?) ON CONFLICT(batch_hash) DO UPDATE SET attempts=attempts+1,last_seen_at_ms=excluded.last_seen_at_ms")
            .bind(hash).bind(source).bind(owner).bind(count as i64).bind(summaries.to_string()).bind(now).bind(now).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn desktop_diagnostics(&self, before: i64) -> Result<Vec<Value>> {
        let rows:Vec<DiagnosticRow>=sqlx::query_as("SELECT id,source,virtual_account_id,record_count,summaries_json,attempts,first_seen_at_ms,last_seen_at_ms FROM desktop_diagnostics WHERE id<? ORDER BY id DESC LIMIT 100").bind(before).fetch_all(self.pool()).await?;
        rows.into_iter().map(|row|Ok(json!({"id":row.id,"source":row.source,"owner":row.virtual_account_id,"record_count":row.record_count,"summaries":serde_json::from_str::<Value>(&row.summaries_json)?,"attempts":row.attempts,"first_seen_at_ms":row.first_seen_at_ms,"last_seen_at_ms":row.last_seen_at_ms}))).collect()
    }
    pub async fn desktop_diagnostic_page(
        &self,
        page: u32,
        page_size: Option<u32>,
    ) -> Result<Value> {
        let page_size = crate::table_page_size(page_size)?;
        let mut tx = self.pool().begin().await?;
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM desktop_diagnostics")
            .fetch_one(&mut *tx)
            .await?;
        let page = i64::from(page).clamp(1, ((total + page_size - 1) / page_size).max(1));
        let rows: Vec<DiagnosticRow> = sqlx::query_as("SELECT id,source,virtual_account_id,record_count,summaries_json,attempts,first_seen_at_ms,last_seen_at_ms FROM desktop_diagnostics ORDER BY id DESC LIMIT ? OFFSET ?")
            .bind(page_size).bind((page-1)*page_size).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        let items: Result<Vec<Value>> = rows.into_iter().map(|row|Ok(json!({"id":row.id,"source":row.source,"owner":row.virtual_account_id,"record_count":row.record_count,"summaries":serde_json::from_str::<Value>(&row.summaries_json)?,"attempts":row.attempts,"first_seen_at_ms":row.first_seen_at_ms,"last_seen_at_ms":row.last_seen_at_ms}))).collect();
        Ok(json!({"items":items?,"total":total,"page":page,"page_size":page_size}))
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

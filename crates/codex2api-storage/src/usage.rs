use crate::{Result, Storage};
use sqlx::{FromRow, QueryBuilder, Sqlite};

#[derive(Clone, Debug, Default, FromRow)]
pub struct UsageRecord {
    pub id: String,
    pub account_id: String,
    pub account_name: String,
    pub api_key_id: String,
    pub api_key_name: String,
    pub endpoint: String,
    pub transport: String,
    pub model: Option<String>,
    pub actual_model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub image_size: Option<String>,
    pub first_byte_ms: Option<i64>,
    pub total_ms: Option<i64>,
    pub requested_at_ms: i64,
    pub status: String,
    pub http_status: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct UsageFilter {
    pub account: Option<String>,
    pub api_key_id: Option<String>,
    pub model: Option<String>,
    pub from_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub status: Option<String>,
    pub page: u32,
}

pub const USAGE_PAGE_SIZE: i64 = 50;
pub struct UsagePage {
    pub records: Vec<UsageRecord>,
    pub total: i64,
    pub page: u32,
}

#[derive(FromRow)]
pub struct UsageKeyOption {
    pub id: String,
    pub name: String,
    pub account_name: String,
}

fn conditions(query: &mut QueryBuilder<'_, Sqlite>, filter: &UsageFilter) {
    query.push(" WHERE 1=1");
    if let Some(account) = filter.account.as_deref().filter(|s| !s.is_empty()) {
        query
            .push(" AND instr(lower(account_name), lower(")
            .push_bind(account.to_string())
            .push(")) > 0");
    }
    if let Some(key) = filter.api_key_id.as_deref().filter(|s| !s.is_empty()) {
        query.push(" AND api_key_id = ").push_bind(key.to_string());
    }
    if let Some(model) = filter.model.as_deref().filter(|s| !s.is_empty()) {
        query
            .push(" AND instr(lower(model), lower(")
            .push_bind(model.to_string())
            .push(")) > 0");
    }
    if let Some(status) = filter.status.as_deref().filter(|s| !s.is_empty()) {
        query.push(" AND status = ").push_bind(status.to_string());
    }
    if let Some(from) = filter.from_ms {
        query.push(" AND requested_at_ms >= ").push_bind(from);
    }
    if let Some(until) = filter.until_ms {
        query.push(" AND requested_at_ms < ").push_bind(until);
    }
}

impl Storage {
    /// Called once by the process at startup; a prior process cannot still own these requests.
    pub async fn recover_interrupted_usage(&self) -> Result<()> {
        sqlx::query("UPDATE usage_records SET status='interrupted' WHERE status='in_progress'")
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn insert_usage(&self, record: &UsageRecord) -> Result<()> {
        sqlx::query("INSERT INTO usage_records (id,account_id,account_name,api_key_id,api_key_name,endpoint,transport,model,reasoning_effort,service_tier,image_size,requested_at_ms,status,actual_model) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&record.id).bind(&record.account_id).bind(&record.account_name).bind(&record.api_key_id).bind(&record.api_key_name)
            .bind(&record.endpoint).bind(&record.transport).bind(&record.model).bind(&record.reasoning_effort).bind(&record.service_tier).bind(&record.image_size).bind(record.requested_at_ms).bind(&record.status)
            .bind(&record.actual_model)
            .execute(self.pool()).await?;
        Ok(())
    }

    pub async fn finish_usage(&self, record: &UsageRecord) -> Result<()> {
        sqlx::query("UPDATE usage_records SET actual_model=?, input_tokens=?,output_tokens=?,cached_tokens=?,cache_write_tokens=?,reasoning_tokens=?,image_size=?,first_byte_ms=?,total_ms=?,status=?,http_status=? WHERE id=? AND status='in_progress'")
            .bind(&record.actual_model).bind(record.input_tokens).bind(record.output_tokens).bind(record.cached_tokens).bind(record.cache_write_tokens)
            .bind(record.reasoning_tokens).bind(&record.image_size).bind(record.first_byte_ms).bind(record.total_ms).bind(&record.status).bind(record.http_status).bind(&record.id)
            .execute(self.pool()).await?;
        Ok(())
    }

    pub async fn query_usage(&self, filter: &UsageFilter) -> Result<UsagePage> {
        let mut tx = self.pool().begin().await?;
        let mut count = QueryBuilder::new("SELECT COUNT(*) FROM usage_records");
        conditions(&mut count, filter);
        let total: i64 = count.build_query_scalar().fetch_one(&mut *tx).await?;
        let max_page = ((total.saturating_sub(1) / USAGE_PAGE_SIZE) + 1).max(1) as u32;
        let page = filter.page.max(1).min(max_page);
        let mut query = QueryBuilder::new("SELECT * FROM usage_records");
        conditions(&mut query, filter);
        query
            .push(" ORDER BY requested_at_ms DESC, id DESC LIMIT ")
            .push_bind(USAGE_PAGE_SIZE)
            .push(" OFFSET ")
            .push_bind(i64::from(page - 1) * USAGE_PAGE_SIZE);
        let records = query
            .build_query_as::<UsageRecord>()
            .fetch_all(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(UsagePage {
            records,
            total,
            page,
        })
    }

    pub async fn usage_key_options(&self) -> Result<Vec<UsageKeyOption>> {
        Ok(sqlx::query_as::<_,UsageKeyOption>("SELECT k.id, COALESCE(NULLIF(k.name,''), k.key_prefix) AS name, COALESCE(NULLIF(a.display_name,''),NULLIF(a.email,''),a.id) AS account_name FROM proxy_api_keys k JOIN accounts a ON a.id=k.account_id UNION ALL SELECT c.id, c.name, COALESCE(NULLIF(a.display_name,''),NULLIF(a.email,''),a.id) AS account_name FROM oauth_credentials c JOIN accounts a ON a.id=c.account_id ORDER BY account_name,name")
            .fetch_all(self.pool()).await?)
    }

    pub async fn usage_account_options(&self) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT DISTINCT COALESCE(NULLIF(display_name,''),NULLIF(email,''),id) AS name FROM accounts ORDER BY name")
            .fetch_all(self.pool()).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn usage_history_persists_filters_paginates_and_keeps_snapshot_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.sqlite");
        let storage = Storage::open(&path).await.unwrap();
        for n in 0..53 {
            let record = UsageRecord {
                id: format!("row-{n:03}"),
                account_id: format!("account-{}", n % 2),
                account_name: if n % 2 == 0 {
                    "Alice %".into()
                } else {
                    "Bob".into()
                },
                api_key_id: format!("key-{}", n % 2),
                api_key_name: format!("name-{}", n % 2),
                endpoint: "/v1/responses".into(),
                transport: "http".into(),
                model: Some("model-test".into()),
                reasoning_effort: Some("xhigh".into()),
                service_tier: Some("priority".into()),
                requested_at_ms: 1000 + n,
                status: "in_progress".into(),
                ..Default::default()
            };
            storage.insert_usage(&record).await.unwrap();
        }
        let first = storage.query_usage(&UsageFilter::default()).await.unwrap();
        assert_eq!(first.total, 53);
        assert_eq!(first.records.len(), 50);
        assert_eq!(first.records[0].id, "row-052");
        let second = storage
            .query_usage(&UsageFilter {
                page: 2,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(second.records.len(), 3);
        assert_eq!(second.records[0].id, "row-002");
        let filtered = storage
            .query_usage(&UsageFilter {
                account: Some("alice %".into()),
                api_key_id: Some("key-0".into()),
                model: Some("MODEL".into()),
                from_ms: Some(1010),
                until_ms: Some(1015),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(filtered.total, 3);
        assert_eq!(
            storage
                .query_usage(&UsageFilter {
                    account: Some("' OR 1=1 --".into()),
                    ..Default::default()
                })
                .await
                .unwrap()
                .total,
            0
        );
        let mut record = filtered.records[0].clone();
        record.status = "completed".into();
        record.input_tokens = Some(0);
        record.first_byte_ms = Some(120);
        record.total_ms = Some(900);
        storage.finish_usage(&record).await.unwrap();
        record.input_tokens = Some(99);
        storage.finish_usage(&record).await.unwrap(); // Finalization is idempotent.
        assert!(storage.usage_key_options().await.unwrap().is_empty());
        let completed = storage
            .query_usage(&UsageFilter {
                status: Some("completed".into()),
                account: Some("lic".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(completed.total, 1);
        assert_eq!(completed.records[0].id, record.id);
        storage.close().await;
        let storage = Storage::open(&path).await.unwrap();
        storage.recover_interrupted_usage().await.unwrap();
        let page = storage
            .query_usage(&UsageFilter {
                from_ms: Some(1014),
                until_ms: Some(1015),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page.records[0].input_tokens, Some(0));
        assert_eq!(page.records[0].reasoning_effort.as_deref(), Some("xhigh"));
        assert_eq!(page.records[0].service_tier.as_deref(), Some("priority"));
        assert_eq!(page.records[0].account_name, "Alice %");
        assert_eq!(page.records[0].total_ms, Some(900));
        let interrupted = storage
            .query_usage(&UsageFilter {
                status: Some("interrupted".into()),
                page: 2,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(interrupted.total, 52);
        assert_eq!(interrupted.records.len(), 2);
        assert!(
            interrupted
                .records
                .iter()
                .all(|r| r.status == "interrupted")
        );
        assert_eq!(
            storage
                .query_usage(&UsageFilter::default())
                .await
                .unwrap()
                .records[0]
                .status,
            "interrupted"
        );
        storage.close().await;
    }

    #[tokio::test]
    async fn deleted_keys_leave_filter_options_but_keep_usage_history() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("options.sqlite"))
            .await
            .unwrap();
        let mut new = crate::NewAccount::pending_identity(
            uuid::Uuid::new_v4().to_string(),
            "codex_cli_rs",
            "ua",
            "Windows",
            "10",
            "x86_64",
            "",
            "{}",
        );
        new.display_name = Some("Alice".into());
        let account = storage.create_account(new).await.unwrap();
        let key = storage
            .create_proxy_api_key(&account.id, Some("test-key"))
            .await
            .unwrap();
        storage
            .insert_usage(&UsageRecord {
                id: "record".into(),
                account_id: account.id.clone(),
                account_name: "Alice".into(),
                api_key_id: key.record.id.clone(),
                api_key_name: "test-key".into(),
                status: "completed".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let options = storage.usage_key_options().await.unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].id, key.record.id);
        assert_eq!(
            storage.usage_account_options().await.unwrap(),
            vec!["Alice"]
        );
        assert!(
            storage
                .delete_proxy_api_key(&account.id, &key.record.id)
                .await
                .unwrap()
        );
        assert!(storage.usage_key_options().await.unwrap().is_empty());
        let history = storage.query_usage(&UsageFilter::default()).await.unwrap();
        assert_eq!(history.total, 1);
        assert_eq!(history.records[0].api_key_name, "test-key");
        storage.close().await;
    }
}

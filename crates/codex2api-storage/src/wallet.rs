use crate::{Result, Storage, StorageError, WalletEntry};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::FromRow;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalletAdjustment {
    pub request_id: String,
    pub amount_cents: i64,
    pub expected_revision: i64,
    pub reason: Option<String>,
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WalletEntryFilter {
    pub user_id: String,
    pub kind: String,
    pub from_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub page: i64,
    pub limit: i64,
}
impl Default for WalletEntryFilter {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            kind: String::new(),
            from_ms: None,
            until_ms: None,
            page: 1,
            limit: 20,
        }
    }
}
#[derive(FromRow, Serialize)]
struct AdminWalletEntry {
    #[sqlx(flatten)]
    #[serde(flatten)]
    entry: WalletEntry,
    user_id: String,
    username: String,
    user_name: String,
}
impl Storage {
    pub async fn admin_wallet_entries(
        &self,
        filter: &WalletEntryFilter,
    ) -> Result<serde_json::Value> {
        if filter.user_id.len() > 128
            || !["", "order_payment", "system_adjustment"].contains(&filter.kind.as_str())
            || filter.page < 1
            || ![10, 20, 30, 50].contains(&filter.limit)
            || matches!((filter.from_ms,filter.until_ms),(Some(from),Some(until)) if from>=until)
            || [filter.from_ms, filter.until_ms]
                .into_iter()
                .flatten()
                .any(|value| !(0..=253_402_300_799_000).contains(&value))
        {
            return Err(StorageError::InvalidAdminUpdate(
                "流水筛选条件或时间范围无效",
            ));
        }
        let offset = (filter.page - 1)
            .checked_mul(filter.limit)
            .ok_or(StorageError::InvalidAdminUpdate("流水页码超出范围"))?;
        let condition = " WHERE (?='' OR w.user_id=?) AND (?='' OR w.kind=?) AND (? IS NULL OR julianday(w.created_at)>=julianday(?/1000.0,'unixepoch')) AND (? IS NULL OR julianday(w.created_at)<julianday(?/1000.0,'unixepoch'))";
        let mut tx = self.pool().begin().await?;
        let total:i64=sqlx::query_scalar(&format!("SELECT COUNT(*) FROM wallet_entries w JOIN user_identities u ON u.id=w.user_id{condition}"))
            .bind(&filter.user_id).bind(&filter.user_id).bind(&filter.kind).bind(&filter.kind).bind(filter.from_ms).bind(filter.from_ms).bind(filter.until_ms).bind(filter.until_ms).fetch_one(&mut *tx).await?;
        let items:Vec<AdminWalletEntry>=sqlx::query_as(&format!("SELECT w.*,u.username,u.name AS user_name FROM wallet_entries w JOIN user_identities u ON u.id=w.user_id{condition} ORDER BY julianday(w.created_at) DESC,w.rowid DESC LIMIT ? OFFSET ?"))
            .bind(&filter.user_id).bind(&filter.user_id).bind(&filter.kind).bind(&filter.kind).bind(filter.from_ms).bind(filter.from_ms).bind(filter.until_ms).bind(filter.until_ms).bind(filter.limit).bind(offset).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"items":items,"total":total,"page":filter.page,"limit":filter.limit}))
    }
    pub async fn adjust_user_wallet(
        &self,
        owner: &str,
        administrator: i64,
        input: WalletAdjustment,
    ) -> Result<WalletEntry> {
        crate::reset_credits::request_id(&input.request_id)?;
        let reason = input
            .reason
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if input.amount_cents == 0 || input.amount_cents == i64::MIN {
            return Err(StorageError::InvalidAdminUpdate("请填写有效的非零调整金额"));
        }
        if reason.is_some_and(|value| value.len() > 1000) {
            return Err(StorageError::InvalidAdminUpdate("调整原因最多 1000 字节"));
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let (actor, actor_name): (String, String) =
            sqlx::query_as("SELECT account_id,username FROM admin_identities WHERE id=?")
                .bind(administrator)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StorageError::InvalidCredentials)?;
        let key = format!("system:{}", input.request_id);
        let signature = crate::hash_token(
            &json!([
                owner,
                actor,
                input.amount_cents,
                input.expected_revision,
                reason
            ])
            .to_string(),
        );
        let prior: Option<WalletEntry> =
            sqlx::query_as("SELECT * FROM wallet_entries WHERE user_id=? AND request_id=?")
                .bind(owner)
                .bind(&key)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(prior) = prior {
            let original: String =
                sqlx::query_scalar("SELECT signature FROM wallet_entries WHERE id=?")
                    .bind(&prior.id)
                    .fetch_one(&mut *tx)
                    .await?;
            if original != signature {
                return Err(StorageError::InvalidAdminUpdate(
                    "此操作标识已使用，请重新打开余额调整窗口",
                ));
            }
            return Ok(prior);
        }
        let (before, revision): (i64, i64) =
            sqlx::query_as("SELECT wallet_cents,revision FROM users WHERE id=?")
                .bind(owner)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| StorageError::AccountNotFound(owner.into()))?;
        if revision != input.expected_revision {
            return Err(StorageError::ProxyChanged);
        }
        let after = before
            .checked_add(input.amount_cents)
            .filter(|v| (0..=9_007_199_254_740_991).contains(v))
            .ok_or(StorageError::InvalidAdminUpdate(
                "减少金额不能超过当前余额，增加后余额不能超出金额范围",
            ))?;
        sqlx::query(
            "UPDATE users SET wallet_cents=?,revision=revision+1 WHERE id=? AND revision=?",
        )
        .bind(after)
        .bind(owner)
        .bind(revision)
        .execute(&mut *tx)
        .await?;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO wallet_entries(id,user_id,request_id,signature,kind,amount_cents,balance_cents,created_at,operator_account_id,operator_name,reason) VALUES(?,?,?,?,'system_adjustment',?,?,?,?,?,?)")
            .bind(&id).bind(owner).bind(key).bind(signature).bind(input.amount_cents).bind(after).bind(chrono::Utc::now().to_rfc3339()).bind(actor).bind(actor_name).bind(reason).execute(&mut *tx).await?;
        let entry = sqlx::query_as("SELECT * FROM wallet_entries WHERE id=?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(entry)
    }
}

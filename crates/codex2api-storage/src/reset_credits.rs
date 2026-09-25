//! Account-owned, single-use quota resets. Never modify the subscription or ledger.
use crate::{Result, Storage, StorageError, VirtualAccount, VirtualPlan};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};

#[derive(sqlx::FromRow)]
struct Credit {
    id: String,
    granted_at_ms: i64,
    redeemed_at_ms: Option<i64>,
    available_at_ms: i64,
    expires_at_ms: Option<i64>,
    source: String,
}

fn timestamp(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms)
        .expect("persisted reset timestamp")
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

impl Credit {
    fn protocol(&self, now: i64) -> Value {
        let status = if self.redeemed_at_ms.is_some() {
            "redeemed"
        } else if self.expires_at_ms.is_some_and(|at| at <= now) {
            "expired"
        } else if self.available_at_ms > now {
            "pending"
        } else {
            "available"
        };
        json!({
            "id":self.id,"reset_type":"codex_rate_limits",
            "status":status,
            "granted_at":timestamp(self.granted_at_ms),"expires_at":self.expires_at_ms.map(timestamp),
            "redeem_started_at":self.redeemed_at_ms.map(timestamp),
            "redeemed_at":self.redeemed_at_ms.map(timestamp),
            "profile_image_url":null,"profile_user_id":null,
            "title":null,"description":null
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UsageRecord;

    async fn fixture() -> (tempfile::TempDir, Storage, i64) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("reset.sqlite"))
            .await
            .unwrap();
        let now = Utc::now().timestamp();
        for id in ["a", "b"] {
            storage
                .save_virtual_account(&VirtualAccount {
                    provider_id: "chatgpt".into(),
                    id: id.into(),
                    username: id.into(),
                    password_hash: "unused".into(),
                    name: id.into(),
                    email: format!("{id}@example.test"),
                    plan_type: "pro".into(),
                    plan_id: "pro".into(),
                    subscription_expires_at: Some(timestamp((now + 86400) * 1000)),
                    enabled: true,
                    created_at: timestamp((now - 3600) * 1000),
                })
                .await
                .unwrap();
        }
        let mut plan = storage.virtual_plan("pro").await.unwrap().unwrap();
        plan.config = json!({"model_access":"all","models":[],"spending_windows":[
            {"duration_seconds":2592000,"cost_limit_usd":"2"},
            {"duration_seconds":18000,"cost_limit_usd":"1"}
        ],"free_access_enabled":false,"free_model_access":"none","free_models":[],"free_spending_windows":[]});
        storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap();
        (dir, storage, now)
    }

    async fn usage(storage: &Storage, owner: &str, id: &str, ms: i64) -> UsageRecord {
        let mut record = UsageRecord {
            id: id.into(),
            subject_id: owner.into(),
            model: Some("gpt-6-astra".into()),
            endpoint: "/v1/responses".into(),
            requested_at_ms: ms,
            status: "in_progress".into(),
            ..Default::default()
        };
        storage.insert_usage(&record).await.unwrap();
        record.input_tokens = Some(100000);
        record.output_tokens = Some(0);
        record.status = "completed".into();
        storage.finish_usage(&record).await.unwrap();
        record
    }

    #[tokio::test]
    async fn reset_restarts_windows_and_preserves_subscription_history_isolation_and_restart() {
        let (dir, storage, now) = fixture().await;
        let a = storage.virtual_account("a").await.unwrap().unwrap();
        let old = usage(&storage, "a", "old", (now - 10) * 1000).await;
        usage(&storage, "b", "other", (now - 10) * 1000).await;
        storage
            .grant_virtual_reset_credits("a", "grant", 2, "private admin note")
            .await
            .unwrap();
        storage
            .grant_virtual_reset_credits("a", "grant", 2, "private admin note")
            .await
            .unwrap();
        let before = storage.virtual_quota("a").await.unwrap();
        assert_eq!(before["rate_limit"]["allowed"], false);
        assert_eq!(before["rate_limit_reset_credits"]["available_count"], 2);
        let response = storage
            .consume_virtual_reset_credit("a", "use", None, "client")
            .await
            .unwrap();
        assert_eq!(response["code"], "reset");
        assert_eq!(response["windows_reset"], 2);
        assert_eq!(response["credit"]["status"], "redeemed");
        let reset_at =
            DateTime::parse_from_rfc3339(response["credit"]["redeemed_at"].as_str().unwrap())
                .unwrap()
                .timestamp();
        let after = storage.virtual_quota("a").await.unwrap();
        assert_eq!(after["rate_limit"]["allowed"], true);
        assert_eq!(after["rate_limit"]["primary_window"]["used_usd"], "0");
        assert!(after["rate_limit"]["secondary_window"].is_null());
        assert_eq!(
            after["rate_limit"]["primary_window"]["reset_at"],
            reset_at + 2592000
        );
        assert_eq!(
            after["rate_limit"]["primary_window"]["started_at"],
            reset_at
        );
        assert_ne!(
            after["rate_limit"]["primary_window"]["reset_at"],
            before["rate_limit"]["secondary_window"]["reset_at"]
        );
        assert_eq!(after["billing"], before["billing"]);
        assert_eq!(
            storage
                .virtual_account("a")
                .await
                .unwrap()
                .unwrap()
                .subscription_expires_at,
            a.subscription_expires_at
        );
        assert_eq!(
            storage.virtual_quota("b").await.unwrap()["rate_limit"]["allowed"],
            false
        );
        // A late completion started before the reset must not refill either window.
        storage.finish_usage(&old).await.unwrap();
        usage(&storage, "a", "late", (now - 5) * 1000).await;
        assert_eq!(
            storage.virtual_quota("a").await.unwrap()["rate_limit"]["primary_window"]["used_usd"],
            "0"
        );
        let list = storage.virtual_reset_credits("a").await.unwrap();
        assert_eq!(list["total_earned_count"], 2);
        assert_eq!(list["available_count"], 1);
        assert!(!list.to_string().contains("private admin note"));
        assert_eq!(
            storage.virtual_reset_credit_records("a").await.unwrap()["items"][0]["note"],
            "private admin note"
        );
        let id = response["credit"]["id"].as_str().unwrap();
        assert_eq!(
            storage
                .consume_virtual_reset_credit("b", "foreign", Some(id), "client")
                .await
                .unwrap()["code"],
            "no_credit"
        );
        storage.close().await;
        let storage = Storage::open(dir.path().join("reset.sqlite"))
            .await
            .unwrap();
        assert_eq!(
            storage
                .consume_virtual_reset_credit("a", "use", None, "client")
                .await
                .unwrap()["code"],
            "already_redeemed"
        );
        assert_eq!(
            storage.virtual_quota("a").await.unwrap()["rate_limit"]["primary_window"]["used_usd"],
            "0"
        );
        usage(&storage, "a", "new", (now + 2) * 1000).await;
        let new = storage.virtual_quota_at("a", now + 2).await.unwrap();
        assert_eq!(new["rate_limit"]["primary_window"]["used_usd"], "1");
        assert_eq!(new["rate_limit"]["secondary_window"]["used_usd"], "1");
        assert_eq!(new["rate_limit"]["primary_window"]["started_at"], now + 2);
        assert_eq!(
            new["rate_limit"]["secondary_window"]["reset_at"],
            after["rate_limit"]["primary_window"]["reset_at"]
        );
    }

    #[tokio::test]
    async fn concurrent_retries_consume_one_card_and_no_op_does_not_spend_cards() {
        let (_dir, storage, now) = fixture().await;
        let empty = storage
            .consume_virtual_reset_credit("a", "no-card", None, "client")
            .await
            .unwrap();
        assert_eq!(empty["code"], "no_credit");
        storage
            .grant_virtual_reset_credits("a", "grant", 2, "")
            .await
            .unwrap();
        assert_eq!(
            storage
                .consume_virtual_reset_credit("a", "empty", None, "client")
                .await
                .unwrap()["code"],
            "nothing_to_reset"
        );
        assert_eq!(storage.virtual_reset_credit_count("a").await.unwrap(), 2);
        usage(&storage, "a", "charge", (now - 1) * 1000).await;
        let (first, retry) = tokio::join!(
            storage.consume_virtual_reset_credit("a", "same", None, "client"),
            storage.consume_virtual_reset_credit("a", "same", None, "client")
        );
        let mut codes = vec![
            first.unwrap()["code"].as_str().unwrap().to_owned(),
            retry.unwrap()["code"].as_str().unwrap().to_owned(),
        ];
        codes.sort();
        assert_eq!(codes, vec!["already_redeemed", "reset"]);
        assert_eq!(storage.virtual_reset_credit_count("a").await.unwrap(), 1);
        assert_eq!(
            storage
                .consume_virtual_reset_credit("a", "no-card", None, "client")
                .await
                .unwrap()["code"],
            "no_credit"
        );
        assert_eq!(
            storage
                .consume_virtual_reset_credit("a", "empty", None, "client")
                .await
                .unwrap()["code"],
            "nothing_to_reset"
        );
        assert!(
            storage
                .consume_virtual_reset_credit("a", " ", None, "client")
                .await
                .is_err()
        );
        assert!(
            storage
                .grant_virtual_reset_credits("a", "grant", 3, "")
                .await
                .is_err()
        );
        assert!(
            storage
                .grant_virtual_reset_credits("a", "bad", 101, "")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn same_millisecond_requests_and_concurrent_selected_cards_keep_exact_usage() {
        let (_dir, storage, now) = fixture().await;
        let mut plan = storage.virtual_plan("pro").await.unwrap().unwrap();
        plan.config["spending_windows"] =
            json!([{ "duration_seconds":604800,"cost_limit_usd":"10" }]);
        storage
            .save_virtual_plan(&plan, Some(plan.revision))
            .await
            .unwrap();
        usage(&storage, "a", "initial", (now - 1) * 1000).await;
        let mut pending = UsageRecord {
            id: "in-flight".into(),
            subject_id: "a".into(),
            model: Some("gpt-6-astra".into()),
            endpoint: "/v1/responses".into(),
            requested_at_ms: now * 1000,
            status: "in_progress".into(),
            ..Default::default()
        };
        storage.insert_usage(&pending).await.unwrap();
        storage
            .grant_virtual_reset_credits("a", "grant", 2, "")
            .await
            .unwrap();
        let first = storage
            .consume_virtual_reset_credit("a", "first", None, "client")
            .await
            .unwrap();
        assert_eq!(first["code"], "reset"); // Partial usage can be reset immediately.
        assert_eq!(first["windows_reset"], 1);
        let reset_at =
            DateTime::parse_from_rfc3339(first["credit"]["redeemed_at"].as_str().unwrap())
                .unwrap()
                .timestamp_millis();
        // Both requests have identical millisecond timestamps; insertion relative
        // to the reset transaction determines which quota generation they use.
        sqlx::query("UPDATE usage_records SET requested_at_ms=? WHERE id='in-flight'")
            .bind(reset_at)
            .execute(storage.pool())
            .await
            .unwrap();
        pending.input_tokens = Some(100000);
        pending.output_tokens = Some(0);
        pending.status = "completed".into();
        storage.finish_usage(&pending).await.unwrap();
        assert_eq!(
            storage.virtual_quota("a").await.unwrap()["rate_limit"]["primary_window"]["used_usd"],
            "0"
        );
        usage(&storage, "a", "same-ms-new", reset_at).await;
        let quota = storage.virtual_quota("a").await.unwrap();
        assert_eq!(quota["rate_limit"]["primary_window"]["used_usd"], "1");
        assert_eq!(
            quota["rate_limit"]["primary_window"]["reset_at"],
            reset_at / 1000 + 604800
        );
        let cards = storage.virtual_reset_credits("a").await.unwrap();
        let card = cards["credits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["status"] == "available")
            .unwrap()["id"]
            .as_str()
            .unwrap();
        let (a, b) = tokio::join!(
            storage.consume_virtual_reset_credit("a", "a", Some(card), "client"),
            storage.consume_virtual_reset_credit("a", "b", Some(card), "admin")
        );
        let mut codes = vec![
            a.unwrap()["code"].as_str().unwrap().to_owned(),
            b.unwrap()["code"].as_str().unwrap().to_owned(),
        ];
        codes.sort();
        assert_eq!(codes, vec!["already_redeemed", "reset"]);
        assert_eq!(storage.virtual_reset_credit_count("a").await.unwrap(), 0);
        assert_eq!(
            storage.virtual_quota("a").await.unwrap()["rate_limit"]["primary_window"]["used_usd"],
            "0"
        );
        assert_eq!(
            storage.virtual_quota("a").await.unwrap()["billing"]["used_usd"],
            "3"
        );
    }

    #[tokio::test]
    async fn expiration_cannot_be_overridden_by_a_reset_card() {
        let (_dir, storage, now) = fixture().await;
        usage(&storage, "a", "charge", (now - 1) * 1000).await;
        storage
            .grant_virtual_reset_credits("a", "grant", 1, "")
            .await
            .unwrap();
        let mut account = storage.virtual_account("a").await.unwrap().unwrap();
        account.subscription_expires_at = Some(timestamp((now - 1) * 1000));
        storage.save_virtual_account(&account).await.unwrap();
        assert_eq!(
            storage
                .consume_virtual_reset_credit("a", "expired", None, "client")
                .await
                .unwrap()["code"],
            "nothing_to_reset"
        );
        assert_eq!(storage.virtual_reset_credit_count("a").await.unwrap(), 1);
        assert_eq!(
            storage.virtual_quota("a").await.unwrap()["rate_limit"]["allowed"],
            false
        );
        assert_eq!(
            storage
                .virtual_account("a")
                .await
                .unwrap()
                .unwrap()
                .subscription_expires_at,
            account.subscription_expires_at
        );
    }

    #[tokio::test]
    async fn administrative_reset_is_not_exposed_as_a_client_card() {
        let (_dir, storage, now) = fixture().await;
        usage(&storage, "a", "admin-charge", (now - 1) * 1000).await;
        let result = storage
            .admin_reset_virtual_quota("a", "admin")
            .await
            .unwrap();
        assert_eq!(result["code"], "reset");
        assert_eq!(
            storage.virtual_reset_credits("a").await.unwrap()["credits"],
            json!([])
        );
        assert_eq!(
            storage.virtual_quota("a").await.unwrap()["rate_limit"]["primary_window"]["used_usd"],
            "0"
        );
        assert_eq!(
            storage.virtual_reset_credit_records("a").await.unwrap()["items"][0]["source"],
            "admin_reset"
        );
    }
}

fn request_id(value: &str) -> Result<()> {
    // Official clients use UUIDs, but the protocol accepts nonempty opaque keys.
    if value.trim().is_empty() || value.len() > 256 {
        return Err(StorageError::InvalidAdminUpdate(
            "请求标识不能为空或超过 256 字节",
        ));
    }
    Ok(())
}

impl Storage {
    pub async fn virtual_reset_credit_count(&self, owner: &str) -> Result<i64> {
        let now = Utc::now().timestamp_millis();
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM virtual_reset_credits WHERE virtual_account_id=? AND redeemed_at_ms IS NULL AND available_at_ms<=? AND (expires_at_ms IS NULL OR expires_at_ms>?) AND source='card'")
            .bind(owner).bind(now).bind(now).fetch_one(self.pool()).await?)
    }

    /// The official list has no pagination; return the account's complete history.
    pub async fn virtual_reset_credits(&self, owner: &str) -> Result<Value> {
        let now = Utc::now().timestamp_millis();
        let credits: Vec<Credit> = sqlx::query_as("SELECT id,granted_at_ms,redeemed_at_ms,available_at_ms,expires_at_ms,source FROM virtual_reset_credits WHERE virtual_account_id=? AND source='card' AND available_at_ms<=? AND (expires_at_ms IS NULL OR expires_at_ms>?) ORDER BY granted_at_ms,id")
            .bind(owner).bind(now).bind(now).fetch_all(self.pool()).await?;
        Ok(
            json!({"available_count":credits.iter().filter(|c|c.redeemed_at_ms.is_none() && c.available_at_ms<=now && c.expires_at_ms.is_none_or(|at|at>now)).count(),
            "total_earned_count":credits.len(),"credits":credits.iter().map(|c|c.protocol(now)).collect::<Vec<_>>()}),
        )
    }

    pub async fn virtual_reset_credit_records(&self, owner: &str) -> Result<Value> {
        let rows: Vec<(String, i64, Option<i64>, Option<String>, i64, String, i64, Option<i64>, String)> = sqlx::query_as(
            "SELECT c.id,c.granted_at_ms,c.redeemed_at_ms,c.redeemed_by,c.windows_reset,g.note,c.available_at_ms,c.expires_at_ms,c.source
             FROM virtual_reset_credits c JOIN virtual_reset_grants g ON g.virtual_account_id=c.virtual_account_id AND g.request_id=c.grant_request_id
             WHERE c.virtual_account_id=? ORDER BY c.granted_at_ms DESC,c.id")
            .bind(owner).fetch_all(self.pool()).await?;
        let now = Utc::now().timestamp_millis();
        Ok(
            json!({"available_count":rows.iter().filter(|r|r.2.is_none() && r.6<=now && r.7.is_none_or(|at|at>now) && r.8=="card").count(),"items":rows.into_iter().map(|(id,granted,redeemed,actor,windows,note,active,expires,source)|json!({
            "id":id,"status":if redeemed.is_some(){"redeemed"}else if expires.is_some_and(|at|at<=now){"expired"}else if active>now{"pending"}else{"available"},
            "granted_at":timestamp(granted),"redeemed_at":redeemed.map(timestamp),"available_at":timestamp(active),"expires_at":expires.map(timestamp),
            "redeemed_by":actor,"windows_reset":windows,"note":note,"source":source
        })).collect::<Vec<_>>() }),
        )
    }

    pub async fn grant_virtual_reset_credits(
        &self,
        owner: &str,
        grant_id: &str,
        quantity: i64,
        note: &str,
    ) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        self.grant_virtual_reset_credits_scheduled(
            owner,
            grant_id,
            quantity,
            note,
            now,
            Some(now + 30 * 86_400_000),
        )
        .await
    }

    pub async fn grant_virtual_reset_credits_scheduled(
        &self,
        owner: &str,
        grant_id: &str,
        quantity: i64,
        note: &str,
        available_at_ms: i64,
        expires_at_ms: Option<i64>,
    ) -> Result<()> {
        request_id(grant_id)?;
        if !(1..=100).contains(&quantity) || note.chars().count() > 256 {
            return Err(StorageError::InvalidAdminUpdate(
                "每次发放 1 至 100 张，备注最多 256 字",
            ));
        }
        if available_at_ms < 0 || expires_at_ms.is_some_and(|at| at <= available_at_ms) {
            return Err(StorageError::InvalidAdminUpdate(
                "重置卡启用时间和有效时长无效",
            ));
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM virtual_accounts WHERE id=?)")
                .bind(owner)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(StorageError::AccountNotFound(owner.into()));
        }
        let previous: Option<(i64, String)> = sqlx::query_as("SELECT quantity,note FROM virtual_reset_grants WHERE virtual_account_id=? AND request_id=?")
            .bind(owner).bind(grant_id).fetch_optional(&mut *tx).await?;
        if let Some(previous) = previous {
            if previous != (quantity, note.to_owned()) {
                return Err(StorageError::InvalidAdminUpdate("重试发卡时不能修改原请求"));
            }
        } else {
            let now = Utc::now().timestamp_millis();
            sqlx::query("INSERT INTO virtual_reset_grants VALUES(?,?,?,?,?,?,?)")
                .bind(owner)
                .bind(grant_id)
                .bind(quantity)
                .bind(note)
                .bind(now)
                .bind(available_at_ms)
                .bind(expires_at_ms)
                .execute(&mut *tx)
                .await?;
            for _ in 0..quantity {
                sqlx::query("INSERT INTO virtual_reset_credits(id,virtual_account_id,grant_request_id,granted_at_ms,available_at_ms,expires_at_ms) VALUES(?,?,?,?,?,?)")
                    .bind(uuid::Uuid::new_v4().to_string()).bind(owner).bind(grant_id).bind(now).bind(available_at_ms).bind(expires_at_ms).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn consume_virtual_reset_credit(
        &self,
        owner: &str,
        redeem_id: &str,
        credit_id: Option<&str>,
        actor: &str,
    ) -> Result<Value> {
        request_id(redeem_id)?;
        if let Some(id) = credit_id {
            request_id(id)?;
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let previous: Option<String> = sqlx::query_scalar("SELECT response_json FROM virtual_reset_requests WHERE virtual_account_id=? AND request_id=?")
            .bind(owner).bind(redeem_id).fetch_optional(&mut *tx).await?;
        if let Some(previous) = previous {
            let mut response: Value = serde_json::from_str(&previous)?;
            if response["code"] == "reset" {
                response["code"] = json!("already_redeemed");
                response["windows_reset"] = json!(0);
            }
            tx.commit().await?;
            return Ok(response);
        }
        let now = Utc::now().timestamp_millis();
        let account: VirtualAccount = sqlx::query_as("SELECT * FROM virtual_accounts WHERE id=?")
            .bind(owner)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(owner.into()))?;
        let mut credit: Option<Credit> = if let Some(id) = credit_id {
            sqlx::query_as("SELECT id,granted_at_ms,redeemed_at_ms,available_at_ms,expires_at_ms,source FROM virtual_reset_credits WHERE virtual_account_id=? AND id=?")
                .bind(owner).bind(id).fetch_optional(&mut *tx).await?
        } else {
            sqlx::query_as("SELECT id,granted_at_ms,redeemed_at_ms,available_at_ms,expires_at_ms,source FROM virtual_reset_credits WHERE virtual_account_id=? AND redeemed_at_ms IS NULL AND available_at_ms<=? AND (expires_at_ms IS NULL OR expires_at_ms>?) AND source='card' ORDER BY granted_at_ms,id LIMIT 1")
                .bind(owner).bind(now).bind(now).fetch_optional(&mut *tx).await?
        };
        let mut response = json!({"code":"no_credit","windows_reset":0,"credit":null});
        if let Some(credit) = credit.as_mut() {
            if credit.redeemed_at_ms.is_some() {
                response["code"] = json!("already_redeemed");
                response["credit"] = credit.protocol(now);
            } else if credit.source == "card"
                && (credit.available_at_ms > now
                    || credit.expires_at_ms.is_some_and(|at| at <= now))
            {
                response["code"] = json!("no_credit");
            } else {
                let plan: VirtualPlan = sqlx::query_as("SELECT * FROM virtual_plans WHERE id=?")
                    .bind(&account.plan_id)
                    .fetch_one(&mut *tx)
                    .await?;
                let paid = account.effective_plan_at(now / 1000) != "free";
                let rules = crate::plan_spending_windows(
                    &plan.config,
                    !paid && account.plan_type != "free",
                )?;
                let anchor: i64 = sqlx::query_scalar("SELECT COALESCE(unixepoch(subscription_started_at),unixepoch(created_at)) FROM virtual_accounts WHERE id=?")
                    .bind(owner).fetch_one(&mut *tx).await?;
                let windows = crate::spending_windows::windows_on(
                    &mut tx,
                    owner,
                    &rules,
                    if anchor > now / 1000 { 0 } else { anchor },
                    now / 1000,
                )
                .await?;
                let used = windows
                    .iter()
                    .any(|w| w["used_usd"].as_str().is_some_and(|v| v != "0"));
                // An expired paid subscription cannot regain its benefits by using a card.
                if !account.enabled || !paid || !used {
                    response["code"] = json!("nothing_to_reset");
                } else {
                    let count = windows
                        .iter()
                        .filter(|w| !w["started_at"].is_null())
                        .count() as i64;
                    sqlx::query("UPDATE virtual_reset_credits SET redeemed_at_ms=?,redeemed_by=?,windows_reset=? WHERE virtual_account_id=? AND id=? AND redeemed_at_ms IS NULL")
                        .bind(now).bind(actor).bind(count).bind(owner).bind(&credit.id).execute(&mut *tx).await?;
                    sqlx::query("UPDATE virtual_accounts SET quota_reset_credit_id=? WHERE id=?")
                        .bind(&credit.id)
                        .bind(owner)
                        .execute(&mut *tx)
                        .await?;
                    credit.redeemed_at_ms = Some(now);
                    response =
                        json!({"code":"reset","windows_reset":count,"credit":credit.protocol(now)});
                }
            }
        }
        sqlx::query("INSERT INTO virtual_reset_requests VALUES(?,?,?,?)")
            .bind(owner)
            .bind(redeem_id)
            .bind(response.to_string())
            .bind(now)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(response)
    }

    /// Administrative reset without issuing a client-visible card.
    pub async fn admin_reset_virtual_quota(&self, owner: &str, actor: &str) -> Result<Value> {
        let request = format!("admin-reset-{}", uuid::Uuid::new_v4());
        let card = format!("admin-reset-{}", uuid::Uuid::new_v4());
        let now = Utc::now().timestamp_millis();
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM virtual_accounts WHERE id=?)")
                .bind(owner)
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(StorageError::AccountNotFound(owner.into()));
        }
        sqlx::query("INSERT INTO virtual_reset_grants VALUES(?,?,?,?,?,?,?)")
            .bind(owner)
            .bind(&request)
            .bind(1_i64)
            .bind("admin direct reset")
            .bind(now)
            .bind(now)
            .bind(now + 30 * 86_400_000)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO virtual_reset_credits(id,virtual_account_id,grant_request_id,granted_at_ms,available_at_ms,expires_at_ms,source) VALUES(?,?,?,?,?,?,?)")
            .bind(&card).bind(owner).bind(&request).bind(now).bind(now).bind(now+30*86_400_000).bind("admin_reset").execute(&mut *tx).await?;
        tx.commit().await?;
        self.consume_virtual_reset_credit(owner, &request, Some(&card), actor)
            .await
    }
}

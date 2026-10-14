//! Wallet settlement for priced usage, committed with its shared ledger record.
use crate::{Result, StorageError, UsageRecord};
pub(crate) const NANO_PER_CENT: i64 = 10_000_000;

pub(crate) async fn settle_usage_on(
    connection: &mut sqlx::SqliteConnection,
    owner: &str,
    record: &UsageRecord,
    cost: Option<i64>,
) -> Result<i64> {
    let provider = &record.provider_id;
    let mut debit = 0_i64;
    if let Some(cost) = cost.filter(|cost| *cost > 0) {
        sqlx::query("INSERT INTO usage_wallet_state(user_id,pending_nano_usd) VALUES(?,0) ON CONFLICT DO NOTHING").bind(&owner).execute(&mut *connection).await?;
        let (balance,pending):(i64,i64)=sqlx::query_as("SELECT u.wallet_cents,s.pending_nano_usd FROM regular_users u JOIN usage_wallet_state s ON s.user_id=u.id WHERE u.id=?").bind(&owner).fetch_one(&mut *connection).await?;
        let due = pending.checked_add(cost).ok_or(StorageError::Api {
            code: "billing_overflow",
            message: "API 待结算金额超出范围",
        })?;
        // A running response can exceed the remaining balance. Keep its exact
        // debt instead of losing the charge or violating the wallet constraint.
        debit = (due / NANO_PER_CENT).min(balance);
        let after = balance - debit;
        let pending = due - debit * NANO_PER_CENT;
        sqlx::query("UPDATE usage_wallet_state SET pending_nano_usd=?,last_usage_id=CASE WHEN ?>0 THEN ? ELSE last_usage_id END WHERE user_id=?")
                .bind(pending).bind(cost).bind(&record.id).bind(&owner).execute(&mut *connection).await?;
        if debit > 0 {
            sqlx::query(
                "UPDATE users SET wallet_cents=?,revision=revision+1 WHERE id=? AND kind='regular'",
            )
            .bind(after)
            .bind(&owner)
            .execute(&mut *connection)
            .await?;
            sqlx::query("INSERT INTO wallet_entries(id,user_id,request_id,signature,kind,provider_id,amount_cents,balance_cents,created_at,usage_record_id) VALUES(?,?,?,?,'usage',?,?,?,?,?)")
                    .bind(uuid::Uuid::new_v4().to_string()).bind(&owner).bind(format!("usage:{}",record.id))
                    .bind(crate::hash_token(&format!("{}:{cost}:{debit}",record.id))).bind(&provider).bind(-debit).bind(after).bind(chrono::Utc::now().to_rfc3339()).bind(&record.id).execute(&mut *connection).await?;
        }
    }
    Ok(debit)
}

/// Replenished wallets repay measured API debt without inventing a new generation.
/// Each debit references the real request that contributed the latest carried cost.
pub(crate) async fn settle_usage_debt_on(
    connection: &mut sqlx::SqliteConnection,
    owner: &str,
) -> Result<()> {
    let state:Option<(i64,i64,Option<String>)>=sqlx::query_as("SELECT u.wallet_cents,s.pending_nano_usd,s.last_usage_id FROM regular_users u JOIN usage_wallet_state s ON s.user_id=u.id WHERE u.id=?")
        .bind(owner).fetch_optional(&mut *connection).await?;
    let Some((balance, pending, request_id)) = state else {
        return Ok(());
    };
    let debit = (pending / NANO_PER_CENT).min(balance);
    if debit == 0 {
        return Ok(());
    }
    let request_id = request_id.ok_or(StorageError::Api {
        code: "billing_record_missing",
        message: "API 待扣款来源记录缺失",
    })?;
    let provider: String =
        sqlx::query_scalar("SELECT provider_id FROM usage_records WHERE id=? AND user_id=?")
            .bind(&request_id)
            .bind(owner)
            .fetch_one(&mut *connection)
            .await?;
    let after = balance - debit;
    sqlx::query(
        "UPDATE users SET wallet_cents=?,revision=revision+1 WHERE id=? AND kind='regular'",
    )
    .bind(after)
    .bind(owner)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE usage_wallet_state SET pending_nano_usd=? WHERE user_id=?")
        .bind(pending - debit * NANO_PER_CENT)
        .bind(owner)
        .execute(&mut *connection)
        .await?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO wallet_entries(id,user_id,request_id,signature,kind,provider_id,amount_cents,balance_cents,created_at,usage_record_id) VALUES(?,?,?,?,'usage',?,?,?,?,?)")
        .bind(&id).bind(owner).bind(format!("usage-debt:{id}")).bind(crate::hash_token(&format!("{owner}:{request_id}:{pending}:{balance}")))
        .bind(provider).bind(-debit).bind(after).bind(chrono::Utc::now().to_rfc3339()).bind(request_id).execute(connection).await?;
    Ok(())
}

//! One bounded writer for request bookkeeping. Every acknowledgement follows commit.
use crate::{Result, Storage, StorageError, UsageRecord};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, oneshot};

type QuotaUpdate = Box<dyn FnOnce(Option<crate::QuotaSnapshot>) -> serde_json::Value + Send>;

pub(crate) enum Write {
    TouchAccess {
        hash: String,
        at: chrono::DateTime<chrono::Utc>,
    },
    Reroute {
        id: String,
        supplier: String,
        name: String,
    },
    Start {
        record: Box<UsageRecord>,
        prices: Arc<String>,
    },
    Finish(Box<UsageRecord>),
    Touch {
        id: String,
        at: String,
    },
    Activity {
        owner: String,
        device: String,
        method: String,
        path: String,
        status: u16,
        duration: i64,
        at: i64,
    },
    Routing {
        id: String,
        revision: i64,
        json: String,
        at: chrono::DateTime<chrono::Utc>,
    },
    Quota {
        id: String,
        revision: i64,
        at: chrono::DateTime<chrono::Utc>,
        update: Option<QuotaUpdate>,
    },
}
struct Pending {
    write: Write,
    reply: oneshot::Sender<Result<u64>>,
}

pub(crate) struct RequestWriter {
    sender: mpsc::Sender<Pending>,
}
impl std::fmt::Debug for RequestWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RequestWriter")
    }
}

impl Storage {
    pub(crate) async fn write_request(&self, write: Write) -> Result<u64> {
        let writer = self
            .request_writer
            .get_or_init(|| async {
                let (sender, receiver) = mpsc::channel(4096);
                tokio::spawn(run(self.pool().clone(), self.runtime.clone(), receiver));
                RequestWriter { sender }
            })
            .await;
        let (reply, result) = oneshot::channel();
        writer
            .sender
            .send(Pending { write, reply })
            .await
            .map_err(|_| StorageError::LedgerUnavailable)?;
        result.await.map_err(|_| StorageError::LedgerUnavailable)?
    }
}

async fn run(
    pool: sqlx::SqlitePool,
    runtime: Arc<crate::runtime::Runtime>,
    mut receiver: mpsc::Receiver<Pending>,
) {
    let mut prices = HashMap::new();
    while let Some(first) = receiver.recv().await {
        // Give simultaneously ready producers one scheduling turn, without a timer delay.
        tokio::task::yield_now().await;
        let mut batch = vec![first];
        while batch.len() < 128 {
            let Ok(next) = receiver.try_recv() else {
                break;
            };
            batch.push(next);
        }
        let mut tx = match pool.begin_with("BEGIN IMMEDIATE").await {
            Ok(tx) => tx,
            Err(error) => {
                tracing::error!(%error, "request journal transaction unavailable");
                for pending in batch {
                    let _ = pending.reply.send(Err(StorageError::LedgerUnavailable));
                }
                continue;
            }
        };
        let mut results = Vec::with_capacity(batch.len());
        for pending in &mut batch {
            results.push(execute(&mut tx, &mut pending.write, &mut prices).await);
        }
        let committed = tx.commit().await;
        if let Err(error) = &committed {
            tracing::error!(%error, "request journal commit failed");
        }
        if committed.is_ok() {
            let mut finished = std::collections::HashSet::new();
            for (pending, result) in batch.iter().zip(&results) {
                if !result.as_ref().is_ok_and(|changed| *changed > 0) {
                    continue;
                }
                match &pending.write {
                    Write::Start { record, .. } => runtime.record_spending_use(record),
                    Write::Finish(record) => {
                        finished.insert(record.subject_id.as_str());
                    }
                    _ => {}
                }
            }
            for owner in finished {
                runtime.advance_version(&format!("spending-decision:{owner}"));
            }
        }
        for (pending, result) in batch.into_iter().zip(results) {
            let result = if committed.is_ok() {
                result
            } else {
                Err(StorageError::LedgerUnavailable)
            };
            let _ = pending.reply.send(result);
        }
    }
}

async fn execute(
    connection: &mut sqlx::SqliteConnection,
    write: &mut Write,
    prices: &mut HashMap<String, Arc<crate::billing::BillingSnapshot>>,
) -> Result<u64> {
    match write {
        Write::TouchAccess { hash, at } => Ok(sqlx::query("UPDATE virtual_devices SET last_used_at=? WHERE id IN(SELECT device_id FROM virtual_access_tokens WHERE token_hash=? AND expires_at>?)")
            .bind(at.to_rfc3339()).bind(hash.as_str()).bind(at.timestamp()).execute(connection).await?.rows_affected()),
        Write::Reroute { id, supplier, name } => Ok(sqlx::query("UPDATE usage_records SET account_id=?,account_name=? WHERE id=? AND status='in_progress'")
            .bind(supplier.as_str()).bind(name.as_str()).bind(id.as_str()).execute(connection).await?.rows_affected()),
        Write::Start { record, prices } => insert_usage_on(connection, record, prices).await,
        Write::Finish(record) => finish_usage_on(connection, record, prices).await,
        Write::Touch { id, at } => Ok(sqlx::query("UPDATE supplier_accounts SET last_used_at=? WHERE id=?").bind(at.as_str()).bind(id.as_str()).execute(connection).await?.rows_affected()),
        Write::Activity { owner, device, method, path, status, duration, at } => {
            Ok(sqlx::query("INSERT INTO virtual_request_logs(virtual_account_id,device_id,method,path,status,duration_ms,created_at_ms) SELECT id,?,?,?,?,?,? FROM platform_accounts WHERE id=?")
                .bind(device.as_str()).bind(method.as_str()).bind(path.as_str()).bind(i64::from(*status)).bind(*duration).bind(*at).bind(owner.as_str()).execute(connection).await?.rows_affected())
        }
        Write::Routing { id, revision, json, at } => {
            Ok(sqlx::query("INSERT INTO supplier_info_cache(account_id,section,response_json,observed_at,auth_revision) SELECT id,'details',?,?,auth_revision FROM supplier_accounts WHERE id=? AND auth_revision=? ON CONFLICT(account_id,section) DO UPDATE SET response_json=excluded.response_json,observed_at=excluded.observed_at,auth_revision=excluded.auth_revision")
                .bind(json.as_str()).bind(*at).bind(id.as_str()).bind(*revision).execute(connection).await?.rows_affected())
        }
        Write::Quota { id, revision, at, update } => {
            let row: Option<(Option<String>, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
                "SELECT q.response_json,q.observed_at FROM supplier_accounts a LEFT JOIN account_quota_cache q ON q.account_id=a.id WHERE a.id=? AND a.auth_revision=?",
            ).bind(id.as_str()).bind(*revision).fetch_optional(&mut *connection).await?;
            let Some((json, observed_at)) = row else { return Ok(0); };
            if observed_at.is_some_and(|observed| observed > *at) { return Ok(0); }
            let previous = json.zip(observed_at).map(|(json, observed_at)| {
                Ok::<_, StorageError>(crate::QuotaSnapshot { value: serde_json::from_str(&json)?, observed_at })
            }).transpose()?;
            let value = update.take().expect("quota observation executes once")(previous);
            Ok(sqlx::query("INSERT INTO account_quota_cache(account_id,response_json,observed_at) VALUES(?,?,?) ON CONFLICT(account_id) DO UPDATE SET response_json=excluded.response_json,observed_at=excluded.observed_at")
                .bind(id.as_str()).bind(serde_json::to_string(&value)?).bind(*at).execute(connection).await?.rows_affected())
        }
    }
}

/// The single insert path for OAuth, API-key, HTTP and WebSocket usage.
pub(crate) async fn insert_usage_on(
    connection: &mut sqlx::SqliteConnection,
    record: &UsageRecord,
    prices: &str,
) -> Result<u64> {
    let source_name: Option<String> = if record.subject_kind == "virtual_account" {
        sqlx::query_scalar(
            "SELECT p.name FROM platform_accounts a JOIN virtual_plans p ON p.provider_id=a.provider_id AND p.id=CASE
             WHEN a.plan_type!='free' AND a.subscription_expires_at IS NOT NULL
                AND (unixepoch(a.subscription_expires_at) IS NULL OR unixepoch(a.subscription_expires_at)<=?)
             THEN (SELECT plan_id FROM platform_free_plans WHERE provider_id=a.provider_id)
             ELSE a.plan_id END WHERE a.id=? AND a.provider_id=?"
        )
        .bind(record.requested_at_ms / 1000).bind(&record.subject_id).bind(&record.provider_id)
        .fetch_optional(&mut *connection).await?
    } else {
        record.group_name.clone()
    };
    Ok(sqlx::query("INSERT INTO usage_records (provider_id,subject_kind,id,account_id,account_name,subject_id,subject_name,endpoint,transport,model,reasoning_effort,service_tier,image_size,image_input_usage_json,requested_at_ms,status,actual_model,pricing_snapshot_json,billing_status,error_code,error_message,upstream_request_id,quota_reset_credit_id,user_id,key_id,group_id,group_name,wallet_user_id,multiplier_micros,stream,source_name) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'pending',?,?,?,(SELECT quota_reset_credit_id FROM platform_accounts WHERE id=?),COALESCE(?,(SELECT user_id FROM regular_platforms WHERE id=?)),?,?,?,?,?,?,?)")
        .bind(&record.provider_id).bind(&record.subject_kind).bind(&record.id).bind(&record.account_id).bind(&record.account_name).bind(&record.subject_id).bind(&record.subject_name)
        .bind(&record.endpoint).bind(&record.transport).bind(&record.model).bind(&record.reasoning_effort).bind(&record.service_tier).bind(&record.image_size).bind(&record.image_input_usage_json).bind(record.requested_at_ms).bind(&record.status)
        .bind(&record.actual_model).bind(prices).bind(&record.error_code).bind(&record.error_message).bind(&record.upstream_request_id).bind(&record.subject_id)
        .bind(&record.user_id).bind(&record.subject_id).bind(&record.key_id).bind(&record.group_id).bind(&record.group_name).bind(&record.wallet_user_id).bind(record.multiplier_micros).bind(record.stream)
        .bind(source_name)
        .execute(connection).await?.rows_affected())
}

#[derive(sqlx::FromRow)]
struct SavedUsage {
    status: String,
    subject_kind: String,
    provider_id: String,
    endpoint: String,
    model: Option<String>,
    service_tier: Option<String>,
    multiplier_micros: i64,
    wallet_user_id: Option<String>,
    pricing_snapshot_json: Option<String>,
}

async fn finish_usage_on(
    connection: &mut sqlx::SqliteConnection,
    record: &UsageRecord,
    prices: &mut HashMap<String, Arc<crate::billing::BillingSnapshot>>,
) -> Result<u64> {
    use sqlx::Connection;
    // A failed settlement must roll back its wallet and ledger changes even in a batch.
    let mut tx = connection.begin().await?;
    let saved: Option<SavedUsage> = sqlx::query_as("SELECT status,subject_kind,provider_id,endpoint,model,service_tier,multiplier_micros,wallet_user_id,pricing_snapshot_json FROM usage_records WHERE id=?")
        .bind(&record.id).fetch_optional(&mut *tx).await?;
    let Some(saved) = saved else {
        return Err(StorageError::Constraint("用量记录不存在".into()));
    };
    if saved.status != "in_progress" {
        return Ok(0);
    }
    if record.provider_id != saved.provider_id
        || record.endpoint != saved.endpoint
        || record.model != saved.model
    {
        return Err(StorageError::Constraint("用量结算与原始请求不一致".into()));
    }
    let snapshot = match saved.pricing_snapshot_json {
        Some(json) => {
            if let Some(value) = prices.get(&json) {
                Some(value.clone())
            } else {
                let value = Arc::new(serde_json::from_str::<crate::billing::BillingSnapshot>(
                    &json,
                )?);
                if prices.len() >= 32
                    && let Some(key) = prices.keys().next().cloned()
                {
                    prices.remove(&key);
                }
                prices.insert(json, value.clone());
                Some(value)
            }
        }
        None => None,
    };
    let mut billable = record.clone();
    billable.subject_kind = saved.subject_kind;
    billable.service_tier = record.service_tier.clone().or(saved.service_tier);
    let not_dispatched = record.account_id.is_empty()
        && record.http_status.is_none()
        && record.first_byte_ms.is_none();
    let rejected = record.first_byte_ms.is_none()
        && record.http_status.is_some_and(|status| {
            matches!(
                status,
                400 | 401 | 402 | 403 | 404 | 405 | 413 | 415 | 422 | 429
            )
        });
    let unused = record.status == "failed"
        && record.input_tokens.is_none()
        && record.output_tokens.is_none()
        && record.cached_tokens.is_none_or(|n| n == 0)
        && record.cache_write_tokens.is_none_or(|n| n == 0)
        && record.reasoning_tokens.is_none_or(|n| n == 0)
        && record.image_count.is_none_or(|n| n == 0)
        && record.image_usage_json.is_none()
        && (not_dispatched || rejected);
    let (base, cost, billing_status, billing_model, billing_tier) = if unused {
        (Some(0), Some(0), "not_charged", record.model.clone(), None)
    } else {
        let (base, _, _, _) = snapshot
            .as_ref()
            .map_or((None, "legacy", None, None), |prices| {
                prices.base_charge(&billable)
            });
        let (cost, status, model, tier) = snapshot
            .as_ref()
            .map_or((None, "legacy", None, None), |prices| {
                prices.charge_scaled(&billable, saved.multiplier_micros)
            });
        (base, cost, status, model, tier)
    };
    let debit = if let Some(owner) = saved.wallet_user_id {
        crate::usage_wallet::settle_usage_on(&mut tx, &owner, record, cost).await?
    } else {
        0
    };
    let changed = sqlx::query("UPDATE usage_records SET account_id=?,account_name=?,base_cost_nano_usd=?,cost_nano_usd=?,billing_status=?,billing_model=?,billing_tier=?,service_tier=?,actual_model=?,input_tokens=?,output_tokens=?,cached_tokens=?,cache_write_tokens=?,reasoning_tokens=?,image_size=?,image_count=?,image_usage_json=?,image_input_usage_json=?,first_byte_ms=?,total_ms=?,status=?,http_status=?,failure_kind=?,failure_status=?,error_code=?,error_message=?,upstream_request_id=?,wallet_debit_cents=?,completed_at_ms=? WHERE id=? AND status='in_progress'")
        .bind(&record.account_id).bind(&record.account_name).bind(base).bind(cost).bind(billing_status).bind(billing_model).bind(billing_tier).bind(&billable.service_tier).bind(&record.actual_model)
        .bind(record.input_tokens).bind(record.output_tokens).bind(record.cached_tokens).bind(record.cache_write_tokens).bind(record.reasoning_tokens).bind(&record.image_size).bind(record.image_count).bind(&record.image_usage_json).bind(&record.image_input_usage_json)
        .bind(record.first_byte_ms).bind(record.total_ms).bind(&record.status).bind(record.http_status).bind(&record.failure_kind).bind(record.failure_status).bind(&record.error_code).bind(&record.error_message).bind(&record.upstream_request_id)
        .bind(debit).bind(chrono::Utc::now().timestamp_millis()).bind(&record.id).execute(&mut *tx).await?.rows_affected();
    tx.commit().await?;
    Ok(changed)
}

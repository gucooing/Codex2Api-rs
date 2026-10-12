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
        Write::Start { record, prices } => {
            Ok(sqlx::query("INSERT INTO usage_records (provider_id,subject_kind,id,account_id,account_name,subject_id,subject_name,endpoint,transport,model,reasoning_effort,service_tier,image_size,image_input_usage_json,requested_at_ms,status,actual_model,pricing_snapshot_json,billing_status,error_code,error_message,upstream_request_id,quota_reset_credit_id) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'pending',?,?,?,(SELECT quota_reset_credit_id FROM platform_accounts WHERE id=?))")
                .bind(&record.provider_id).bind(&record.subject_kind).bind(&record.id).bind(&record.account_id).bind(&record.account_name).bind(&record.subject_id).bind(&record.subject_name)
                .bind(&record.endpoint).bind(&record.transport).bind(&record.model).bind(&record.reasoning_effort).bind(&record.service_tier).bind(&record.image_size).bind(&record.image_input_usage_json).bind(record.requested_at_ms).bind(&record.status)
                .bind(&record.actual_model).bind(prices.as_str()).bind(&record.error_code).bind(&record.error_message).bind(&record.upstream_request_id).bind(&record.subject_id)
                .execute(&mut *connection).await?.rows_affected())
        }
        Write::Finish(record) => {
            let snapshot: Option<Option<String>> = sqlx::query_scalar("SELECT pricing_snapshot_json FROM usage_records WHERE id=?")
                .bind(&record.id).fetch_optional(&mut *connection).await?;
            let snapshot = match snapshot.flatten() {
                Some(json) => {
                    if let Some(value) = prices.get(&json) { Some(value.clone()) }
                    else {
                        let value = Arc::new(serde_json::from_str::<crate::billing::BillingSnapshot>(&json)?);
                        if prices.len() >= 32 && let Some(key) = prices.keys().next().cloned() { prices.remove(&key); }
                        prices.insert(json, value.clone());
                        Some(value)
                    }
                },
                None => None,
            };
            let (cost, billing_status, billing_model, billing_tier) = snapshot.as_ref().map_or((None, "legacy", None, None), |prices| prices.charge(record));
            Ok(sqlx::query("UPDATE usage_records SET cost_nano_usd=?,billing_status=?,billing_model=?,billing_tier=?,service_tier=?,actual_model=?, input_tokens=?,output_tokens=?,cached_tokens=?,cache_write_tokens=?,reasoning_tokens=?,image_size=?,image_count=?,image_usage_json=?,image_input_usage_json=?,first_byte_ms=?,total_ms=?,status=?,http_status=?,failure_kind=?,failure_status=?,error_code=?,error_message=?,upstream_request_id=? WHERE id=? AND status='in_progress'")
                .bind(cost).bind(billing_status).bind(billing_model).bind(billing_tier).bind(&record.service_tier).bind(&record.actual_model).bind(record.input_tokens).bind(record.output_tokens).bind(record.cached_tokens).bind(record.cache_write_tokens)
                .bind(record.reasoning_tokens).bind(&record.image_size).bind(record.image_count).bind(&record.image_usage_json).bind(&record.image_input_usage_json).bind(record.first_byte_ms).bind(record.total_ms).bind(&record.status).bind(record.http_status).bind(&record.failure_kind).bind(record.failure_status).bind(&record.error_code).bind(&record.error_message).bind(&record.upstream_request_id).bind(&record.id)
                .execute(&mut *connection).await?.rows_affected())
        }
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

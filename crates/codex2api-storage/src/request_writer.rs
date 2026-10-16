//! Independent PostgreSQL transactions for request bookkeeping. Acknowledgements follow commit.
use crate::{Result, Storage, StorageError, UsageRecord};
use std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};

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
    Rejected(Box<UsageRecord>),
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
pub(crate) type PriceCache = Mutex<HashMap<String, Arc<crate::billing::BillingSnapshot>>>;

impl Storage {
    pub(crate) async fn write_request(&self, mut write: Write) -> Result<u64> {
        // Pool acquisition supplies bounded backpressure. Independent owners never
        // wait behind a process-wide writer or share another request's rollback.
        if matches!(
            write,
            Write::TouchAccess { .. }
                | Write::Reroute { .. }
                | Write::Touch { .. }
                | Write::Activity { .. }
        ) {
            return execute(
                &mut *self.pool().acquire().await?,
                &mut write,
                &self.request_prices,
            )
            .await;
        }
        let mut tx = self.pool().begin().await?;
        let changed = execute(&mut tx, &mut write, &self.request_prices).await?;
        tx.commit().await?;
        if changed > 0 {
            match &write {
                Write::Start { record, .. } => self.runtime.record_spending_use(record),
                Write::Finish(record) if record.subject_kind != "api_key" => {
                    self.runtime
                        .snapshots
                        .invalidate_key(&format!("spending-decision:{}", record.subject_id));
                }
                _ => {}
            }
        }
        Ok(changed)
    }
}

async fn execute(
    connection: &mut sqlx::PgConnection,
    write: &mut Write,
    prices: &PriceCache,
) -> Result<u64> {
    match write {
        Write::TouchAccess { hash, at } => Ok(sqlx::query("UPDATE virtual_devices SET last_used_at=$1 WHERE id IN(SELECT device_id FROM virtual_access_tokens WHERE token_hash=$2 AND expires_at>$3)")
            .bind(at.to_rfc3339()).bind(hash.as_str()).bind(at.timestamp()).execute(connection).await?.rows_affected()),
        Write::Reroute { id, supplier, name } => Ok(sqlx::query("UPDATE usage_records SET account_id=$1,account_name=$2 WHERE id=$3 AND status='in_progress'")
            .bind(supplier.as_str()).bind(name.as_str()).bind(id.as_str()).execute(connection).await?.rows_affected()),
        Write::Start { record, prices } => insert_usage_on(connection, record, Some(prices)).await,
        Write::Finish(record) => finish_usage_on(connection, record, prices).await,
        Write::Rejected(record) => {
            let mut pending = record.as_ref().clone();
            pending.status = "in_progress".into();
            insert_usage_on(connection, &pending, None).await?;
            let changed = sqlx::query("UPDATE usage_records SET status='failed',http_status=$1,total_ms=$2,completed_at_ms=$3,billing_status='not_charged',base_cost_nano_usd=0,cost_nano_usd=0 WHERE id=$4")
                .bind(record.http_status).bind(record.total_ms).bind(chrono::Utc::now().timestamp_millis()).bind(&record.id)
                .execute(&mut *connection).await?.rows_affected();
            Ok(changed)
        }
        Write::Touch { id, at } => Ok(sqlx::query("UPDATE supplier_accounts SET last_used_at=$1 WHERE id=$2").bind(at.as_str()).bind(id.as_str()).execute(connection).await?.rows_affected()),
        Write::Activity { owner, device, method, path, status, duration, at } => {
            Ok(sqlx::query("INSERT INTO virtual_request_logs(virtual_account_id,device_id,method,path,status,duration_ms,created_at_ms) SELECT id,$1,$2,$3,$4,$5,$6 FROM platform_accounts WHERE id=$7")
                .bind(device.as_str()).bind(method.as_str()).bind(path.as_str()).bind(i64::from(*status)).bind(*duration).bind(*at).bind(owner.as_str()).execute(connection).await?.rows_affected())
        }
        Write::Routing { id, revision, json, at } => {
            crate::storage::lock_supplier(connection, id).await?;
            Ok(sqlx::query("INSERT INTO supplier_info_cache(account_id,section,response_json,observed_at,auth_revision) SELECT id,'details',$1,$2,auth_revision FROM supplier_accounts WHERE id=$3 AND auth_revision=$4 ON CONFLICT(account_id,section) DO UPDATE SET response_json=excluded.response_json,observed_at=excluded.observed_at,auth_revision=excluded.auth_revision")
                .bind(json.as_str()).bind(*at).bind(id.as_str()).bind(*revision).execute(connection).await?.rows_affected())
        }
        Write::Quota { id, revision, at, update } => {
            let row: Option<(Option<String>, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
                "SELECT q.response_json,q.observed_at FROM supplier_accounts a LEFT JOIN account_quota_cache q ON q.account_id=a.id WHERE a.id=$1 AND a.auth_revision=$2 FOR UPDATE OF a",
            ).bind(id.as_str()).bind(*revision).fetch_optional(&mut *connection).await?;
            let Some((json, observed_at)) = row else { return Ok(0); };
            if observed_at.is_some_and(|observed| observed > *at) { return Ok(0); }
            let previous = json.zip(observed_at).map(|(json, observed_at)| {
                Ok::<_, StorageError>(crate::QuotaSnapshot::new(serde_json::from_str(&json)?, observed_at))
            }).transpose()?;
            let value = update.take().expect("quota observation executes once")(previous);
            Ok(sqlx::query("INSERT INTO account_quota_cache(account_id,response_json,observed_at) VALUES($1,$2,$3) ON CONFLICT(account_id) DO UPDATE SET response_json=excluded.response_json,observed_at=excluded.observed_at WHERE excluded.observed_at>=account_quota_cache.observed_at")
                .bind(id.as_str()).bind(serde_json::to_string(&value)?).bind(*at).execute(connection).await?.rows_affected())
        }
    }
}

/// The single insert path for OAuth, API-key, HTTP and WebSocket usage.
pub(crate) async fn insert_usage_on(
    connection: &mut sqlx::PgConnection,
    record: &UsageRecord,
    prices: Option<&str>,
) -> Result<u64> {
    if record.subject_kind == "virtual_account" {
        // A reset and admission must agree on which quota generation owns this request.
        sqlx::query("SELECT id FROM platform_accounts WHERE id=$1 FOR SHARE")
            .bind(&record.subject_id)
            .execute(&mut *connection)
            .await?;
    }
    let source_name: Option<String> = if record.subject_kind == "virtual_account" {
        sqlx::query_scalar(
            "SELECT p.name FROM platform_accounts a JOIN virtual_plans p ON p.id=a.plan_id AND p.provider_id=a.provider_id WHERE a.id=$1 AND a.provider_id=$2"
        )
        .bind(&record.subject_id).bind(&record.provider_id)
        .fetch_optional(&mut *connection).await?
    } else {
        record.group_name.clone()
    };
    Ok(sqlx::query("INSERT INTO usage_records (provider_id,subject_kind,id,account_id,account_name,subject_id,subject_name,endpoint,transport,model,reasoning_effort,service_tier,image_size,image_input_usage_json,requested_at_ms,status,actual_model,pricing_snapshot_json,billing_status,error_code,error_message,upstream_request_id,quota_reset_credit_id,user_id,key_id,group_id,group_name,wallet_user_id,multiplier_micros,stream,source_name,user_agent) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,'pending',$19,$20,$21,(SELECT quota_reset_credit_id FROM platform_accounts WHERE id=$22),COALESCE($23,(SELECT user_id FROM regular_platforms WHERE id=$24)),$25,$26,$27,$28,$29,$30,$31,$32)")
        .bind(&record.provider_id).bind(&record.subject_kind).bind(&record.id).bind(&record.account_id).bind(&record.account_name).bind(&record.subject_id).bind(&record.subject_name)
        .bind(&record.endpoint).bind(&record.transport).bind(&record.model).bind(&record.reasoning_effort).bind(&record.service_tier).bind(&record.image_size).bind(&record.image_input_usage_json).bind(record.requested_at_ms).bind(&record.status)
        .bind(&record.actual_model).bind(prices).bind(&record.error_code).bind(&record.error_message).bind(&record.upstream_request_id).bind(&record.subject_id)
        .bind(&record.user_id).bind(&record.subject_id).bind(&record.key_id).bind(&record.group_id).bind(&record.group_name).bind(&record.wallet_user_id).bind(record.multiplier_micros).bind(record.stream)
        .bind(source_name).bind(&record.user_agent)
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
    connection: &mut sqlx::PgConnection,
    record: &UsageRecord,
    prices: &PriceCache,
) -> Result<u64> {
    // The caller owns the transaction: wallet and usage commit or roll back together.
    let saved: Option<SavedUsage> = sqlx::query_as("SELECT status,subject_kind,provider_id,endpoint,model,service_tier,multiplier_micros,wallet_user_id,pricing_snapshot_json FROM usage_records WHERE id=$1 FOR UPDATE")
        .bind(&record.id).fetch_optional(&mut *connection).await?;
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
    let json = saved
        .pricing_snapshot_json
        .ok_or_else(|| StorageError::Constraint("用量记录缺少价格快照".into()))?;
    let snapshot = {
        let mut prices = prices.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(value) = prices.get(&json) {
            value.clone()
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
            value
        }
    };
    let mut billable = record.clone();
    billable.subject_kind = saved.subject_kind;
    billable.service_tier = record.service_tier.clone().or(saved.service_tier);
    let not_dispatched = record.account_id.is_empty();
    let rejected = !record.generation_observed
        && record.http_status.is_some_and(|status| {
            matches!(
                status,
                400 | 401 | 402 | 403 | 404 | 405 | 413 | 415 | 422 | 429
            )
        });
    let unused = record.status == "failed"
        && !record.generation_observed
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
        let (base, _, _, _) = snapshot.base_charge(&billable);
        let (cost, status, model, tier) =
            snapshot.charge_scaled(&billable, saved.multiplier_micros);
        (base, cost, status, model, tier)
    };
    let debit = if let Some(owner) = saved.wallet_user_id {
        crate::usage_wallet::settle_usage_on(connection, &owner, record, cost).await?
    } else {
        0
    };
    let changed = sqlx::query("UPDATE usage_records SET account_id=$1,account_name=$2,base_cost_nano_usd=$3,cost_nano_usd=$4,billing_status=$5,billing_model=$6,billing_tier=$7,service_tier=$8,actual_model=$9,input_tokens=$10,output_tokens=$11,cached_tokens=$12,cache_write_tokens=$13,reasoning_tokens=$14,image_size=$15,image_count=$16,image_usage_json=$17,image_input_usage_json=$18,first_token_ms=$19,total_ms=$20,status=$21,http_status=$22,error_code=$23,error_message=$24,upstream_request_id=$25,wallet_debit_cents=$26,completed_at_ms=$27 WHERE id=$28 AND status='in_progress'")
        .bind(&record.account_id).bind(&record.account_name).bind(base).bind(cost).bind(billing_status).bind(billing_model).bind(billing_tier).bind(&billable.service_tier).bind(&record.actual_model)
        .bind(record.input_tokens).bind(record.output_tokens).bind(record.cached_tokens).bind(record.cache_write_tokens).bind(record.reasoning_tokens).bind(&record.image_size).bind(record.image_count).bind(&record.image_usage_json).bind(&record.image_input_usage_json)
        .bind(record.first_token_ms).bind(record.total_ms).bind(&record.status).bind(record.http_status).bind(&record.error_code).bind(&record.error_message).bind(&record.upstream_request_id)
        .bind(debit).bind(chrono::Utc::now().timestamp_millis()).bind(&record.id).execute(&mut *connection).await?.rows_affected();
    Ok(changed)
}

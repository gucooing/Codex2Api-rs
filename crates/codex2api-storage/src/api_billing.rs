//! API request prices are frozen before execution and settled once with the wallet.
use crate::billing::BillingSnapshot;
use crate::{ApiPrincipal, Result, Storage, StorageError, UsageRecord};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Clone, Debug)]
pub struct ApiRequestStart {
    pub request_id: String,
    pub multiplier: String,
}
#[derive(Clone, Debug, FromRow, Serialize)]
pub struct ApiSettlement {
    pub cost_nano_usd: Option<i64>,
    pub billing_status: String,
    pub wallet_debit_cents: i64,
}

const NANO_PER_CENT: i64 = 10_000_000;

impl Storage {
    /// Run once before binding listeners. A process loss does not prove zero usage.
    pub async fn recover_interrupted_api_usage(&self) -> Result<u64> {
        Ok(sqlx::query("UPDATE api_usage_records SET completed_at_ms=?,status='failed',billing_status='interrupted' WHERE completed_at_ms IS NULL")
            .bind(chrono::Utc::now().timestamp_millis()).execute(self.pool()).await?.rows_affected())
    }

    pub async fn start_api_request(
        &self,
        principal: &ApiPrincipal,
        record: &UsageRecord,
    ) -> Result<ApiRequestStart> {
        let model = record
            .model
            .as_deref()
            .filter(|m| codex2api_core::valid_model(m))
            .ok_or(StorageError::Api {
                code: "invalid_model",
                message: "请指定有效模型",
            })?;
        if record.id.is_empty() || record.id.len() > 128 || record.requested_at_ms <= 0 {
            return Err(StorageError::Api {
                code: "invalid_request",
                message: "请求标识或时间无效",
            });
        }
        let image = matches!(
            record.endpoint.as_str(),
            "/v1/images/generations" | "/v1/images/edits"
        );
        if !image
            && !matches!(
                record.endpoint.as_str(),
                "/v1/responses" | "/v1/chat/completions"
            )
        {
            return Err(StorageError::Api {
                code: "unsupported_endpoint",
                message: "API 端点不受支持",
            });
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let current = crate::api_access::principal_on(&mut tx, &principal.key_id, false)
            .await?
            .filter(|p| p.user_id == principal.user_id && p.group.id == principal.group.id)
            .ok_or(StorageError::Api {
                code: "invalid_api_key",
                message: "API Key 不可用",
            })?;
        if current.group.revision != principal.group.revision {
            return Err(StorageError::Api {
                code: "api_group_changed",
                message: "API 组规则已更改，请重试",
            });
        }
        if current.group.provider_id != record.provider_id {
            return Err(StorageError::Api {
                code: "invalid_model",
                message: "模型平台与 API 组不一致",
            });
        }
        let supported:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM model_catalog m WHERE m.provider_id=? AND m.model=? AND m.enabled=1 AND m.deleted=0 AND m.kind=? AND EXISTS(SELECT 1 FROM api_group_tags gt JOIN supplier_tag_members tm ON tm.tag_id=gt.tag_id JOIN supplier_accounts s ON s.id=tm.account_id JOIN supplier_models sm ON sm.account_id=s.id WHERE gt.group_id=? AND s.provider_id=m.provider_id AND sm.model=m.model))")
            .bind(&current.group.provider_id).bind(model).bind(if image { "image" } else { "text" }).bind(&current.group.id).fetch_one(&mut *tx).await?;
        if !supported
            || (!current.group.allowed_models.is_empty()
                && !current.group.allowed_models.iter().any(|m| m == model))
        {
            return Err(StorageError::Api {
                code: "model_not_available",
                message: "该 API 组不支持此模型",
            });
        }
        let multiplier = current.group.model_multiplier(model).to_owned();
        let multiplier_micros = crate::api_groups::multiplier_micros(&multiplier)?;
        settle_api_debt_on(&mut tx, &current.user_id).await?;
        let (balance,pending):(i64,i64)=sqlx::query_as("SELECT u.wallet_cents,COALESCE(s.pending_nano_usd,0) FROM regular_users u LEFT JOIN api_wallet_state s ON s.user_id=u.id WHERE u.id=?")
            .bind(&current.user_id).fetch_one(&mut *tx).await?;
        if multiplier_micros != 0
            && i128::from(balance) * i128::from(NANO_PER_CENT) <= i128::from(pending)
        {
            // Keep the repayment of already delivered usage even when this new
            // generation cannot be admitted after the wallet was replenished.
            tx.commit().await?;
            return Err(StorageError::Api {
                code: "insufficient_balance",
                message: "钱包余额不足",
            });
        }
        let tokens: Vec<crate::ModelPrice> = sqlx::query_as(
            "SELECT * FROM model_prices WHERE provider_id=? ORDER BY model,tier,min_input_tokens",
        )
        .bind(&current.group.provider_id)
        .fetch_all(&mut *tx)
        .await?;
        let images: Vec<crate::ImagePrice> = sqlx::query_as(
            "SELECT * FROM model_image_prices WHERE provider_id=? ORDER BY model,resolution",
        )
        .bind(&current.group.provider_id)
        .fetch_all(&mut *tx)
        .await?;
        let has_price = if image {
            let size = record.image_size.as_deref().filter(|size| *size != "auto");
            images.iter().any(|p| {
                p.model == model
                    && size.is_none_or(|size| {
                        p.resolution == size
                            || (crate::image_resolution_tier(size).is_some()
                                && crate::image_resolution_tier(&p.resolution).is_some())
                    })
            })
        } else {
            let tier = crate::price_tier(record.service_tier.as_deref());
            tokens.iter().any(|p| {
                p.model == model && Some(p.tier.as_str()) == tier && p.min_input_tokens == 0
            })
        };
        if !has_price {
            return Err(StorageError::Api {
                code: "model_price_unavailable",
                message: "此模型、服务档位或图像尺寸尚未配置价格",
            });
        }
        let snapshot = serde_json::to_string(&BillingSnapshot::Current {
            tokens,
            images,
            operations: vec![],
        })?;
        sqlx::query("INSERT INTO api_usage_records(id,user_id,key_id,group_id,group_name,provider_id,endpoint,model,service_tier,multiplier_micros,pricing_snapshot_json,requested_at_ms,image_size) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&record.id).bind(&current.user_id).bind(&current.key_id).bind(&current.group.id).bind(&current.group.name).bind(&current.group.provider_id)
            .bind(&record.endpoint).bind(model).bind(&record.service_tier).bind(multiplier_micros).bind(snapshot).bind(record.requested_at_ms).bind(&record.image_size).execute(&mut *tx).await?;
        sqlx::query("UPDATE api_keys SET last_used_at=? WHERE id=?")
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(&current.key_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(ApiRequestStart {
            request_id: record.id.clone(),
            multiplier,
        })
    }

    pub async fn finish_api_request(&self, record: &UsageRecord) -> Result<ApiSettlement> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row:Option<(String,String,String,String,Option<String>,i64,String,Option<i64>)>=sqlx::query_as("SELECT user_id,provider_id,endpoint,model,service_tier,multiplier_micros,pricing_snapshot_json,completed_at_ms FROM api_usage_records WHERE id=?")
            .bind(&record.id).fetch_optional(&mut *tx).await?;
        let (owner, provider, endpoint, model, tier, multiplier, snapshot, completed) =
            row.ok_or(StorageError::Api {
                code: "api_request_not_found",
                message: "API 请求记录不存在",
            })?;
        if completed.is_some() {
            return Ok(sqlx::query_as("SELECT cost_nano_usd,billing_status,wallet_debit_cents FROM api_usage_records WHERE id=?").bind(&record.id).fetch_one(&mut *tx).await?);
        }
        if record.provider_id != provider
            || record.endpoint != endpoint
            || record.model.as_deref() != Some(model.as_str())
        {
            return Err(StorageError::Api {
                code: "api_request_mismatch",
                message: "API 请求结算与原始记录不一致",
            });
        }
        let mut billable = record.clone();
        billable.service_tier = record.service_tier.clone().or(tier);
        if endpoint == "/v1/chat/completions" {
            billable.endpoint = "/v1/responses".into();
        }
        let prices: BillingSnapshot = serde_json::from_str(&snapshot)?;
        let not_dispatched = record.account_id.is_empty()
            && record.http_status.is_none()
            && record.first_byte_ms.is_none();
        let rejected_before_generation = record.first_byte_ms.is_none()
            && record.http_status.is_some_and(|status| {
                matches!(
                    status,
                    400 | 401 | 402 | 403 | 404 | 405 | 413 | 415 | 422 | 429
                )
            });
        let failed_without_usage = record.status == "failed"
            && record.input_tokens.is_none()
            && record.output_tokens.is_none()
            && record.cached_tokens.is_none_or(|count| count == 0)
            && record.cache_write_tokens.is_none_or(|count| count == 0)
            && record.reasoning_tokens.is_none_or(|count| count == 0)
            && record.image_count.is_none_or(|count| count == 0)
            && record.image_usage_json.is_none()
            && (not_dispatched || rejected_before_generation);
        let (base, cost, billing_status) = if failed_without_usage {
            (Some(0), Some(0), "not_charged")
        } else {
            let (base, _, _, _) = prices.charge(&billable);
            let (cost, status, _, _) = prices.charge_scaled(&billable, multiplier);
            (base, cost, status)
        };
        let mut debit = 0_i64;
        if let Some(cost) = cost.filter(|cost| *cost > 0) {
            sqlx::query("INSERT INTO api_wallet_state(user_id,pending_nano_usd) VALUES(?,0) ON CONFLICT DO NOTHING").bind(&owner).execute(&mut *tx).await?;
            let (balance,pending):(i64,i64)=sqlx::query_as("SELECT u.wallet_cents,s.pending_nano_usd FROM regular_users u JOIN api_wallet_state s ON s.user_id=u.id WHERE u.id=?").bind(&owner).fetch_one(&mut *tx).await?;
            let due = pending.checked_add(cost).ok_or(StorageError::Api {
                code: "billing_overflow",
                message: "API 待结算金额超出范围",
            })?;
            // A running response can exceed the remaining balance. Keep its exact
            // debt instead of losing the charge or violating the wallet constraint.
            debit = (due / NANO_PER_CENT).min(balance);
            let after = balance - debit;
            let pending = due - debit * NANO_PER_CENT;
            sqlx::query("UPDATE api_wallet_state SET pending_nano_usd=?,last_usage_id=CASE WHEN ?>0 THEN ? ELSE last_usage_id END WHERE user_id=?")
                .bind(pending).bind(cost).bind(&record.id).bind(&owner).execute(&mut *tx).await?;
            if debit > 0 {
                sqlx::query("UPDATE users SET wallet_cents=?,revision=revision+1 WHERE id=? AND kind='regular'").bind(after).bind(&owner).execute(&mut *tx).await?;
                sqlx::query("INSERT INTO wallet_entries(id,user_id,request_id,signature,kind,provider_id,amount_cents,balance_cents,created_at,api_usage_record_id) VALUES(?,?,?,?,'api_usage',?,?,?,?,?)")
                    .bind(uuid::Uuid::new_v4().to_string()).bind(&owner).bind(format!("api:{}",record.id))
                    .bind(crate::hash_token(&format!("{}:{cost}:{debit}",record.id))).bind(&provider).bind(-debit).bind(after).bind(chrono::Utc::now().to_rfc3339()).bind(&record.id).execute(&mut *tx).await?;
            }
        }
        sqlx::query("UPDATE api_usage_records SET supplier_account_id=?,actual_model=?,service_tier=?,input_tokens=?,output_tokens=?,cached_tokens=?,cache_write_tokens=?,reasoning_tokens=?,image_size=?,image_count=?,image_usage_json=?,completed_at_ms=?,status=?,http_status=?,base_cost_nano_usd=?,cost_nano_usd=?,billing_status=?,wallet_debit_cents=? WHERE id=? AND completed_at_ms IS NULL")
            .bind((!record.account_id.is_empty()).then_some(&record.account_id)).bind(&record.actual_model).bind(&billable.service_tier)
            .bind(record.input_tokens).bind(record.output_tokens).bind(record.cached_tokens).bind(record.cache_write_tokens).bind(record.reasoning_tokens)
            .bind(&record.image_size).bind(record.image_count).bind(&record.image_usage_json).bind(chrono::Utc::now().timestamp_millis())
            .bind(&record.status).bind(record.http_status).bind(base).bind(cost).bind(billing_status).bind(debit).bind(&record.id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(ApiSettlement {
            cost_nano_usd: cost,
            billing_status: billing_status.into(),
            wallet_debit_cents: debit,
        })
    }
}

/// Replenished wallets repay measured API debt without inventing a new generation.
/// Each debit references the real request that contributed the latest carried cost.
pub(crate) async fn settle_api_debt_on(
    connection: &mut sqlx::SqliteConnection,
    owner: &str,
) -> Result<()> {
    let state:Option<(i64,i64,Option<String>)>=sqlx::query_as("SELECT u.wallet_cents,s.pending_nano_usd,s.last_usage_id FROM regular_users u JOIN api_wallet_state s ON s.user_id=u.id WHERE u.id=?")
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
        sqlx::query_scalar("SELECT provider_id FROM api_usage_records WHERE id=? AND user_id=?")
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
    sqlx::query("UPDATE api_wallet_state SET pending_nano_usd=? WHERE user_id=?")
        .bind(pending - debit * NANO_PER_CENT)
        .bind(owner)
        .execute(&mut *connection)
        .await?;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO wallet_entries(id,user_id,request_id,signature,kind,provider_id,amount_cents,balance_cents,created_at,api_usage_record_id) VALUES(?,?,?,?,'api_usage',?,?,?,?,?)")
        .bind(&id).bind(owner).bind(format!("api-debt:{id}")).bind(crate::hash_token(&format!("{owner}:{request_id}:{pending}:{balance}")))
        .bind(provider).bind(-debit).bind(after).bind(chrono::Utc::now().to_rfc3339()).bind(request_id).execute(connection).await?;
    Ok(())
}

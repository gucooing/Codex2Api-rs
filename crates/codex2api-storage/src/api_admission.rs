//! API credential, model and wallet admission before the shared usage journal.
use crate::billing::{BillingSnapshot, PriceRequirement};
use crate::usage_wallet::NANO_PER_CENT;
use crate::{ApiPrincipal, Result, Storage, StorageError, UsageRecord};

impl Storage {
    pub async fn admit_api_request(
        &self,
        principal: &ApiPrincipal,
        record: &UsageRecord,
    ) -> Result<()> {
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
        let requirement = match record.endpoint.as_str() {
            "/v1/responses" | "/v1/chat/completions" | "/v1/responses/compact" => {
                PriceRequirement::Tokens(crate::price_tier(record.service_tier.as_deref()))
            }
            "/v1/images/generations" | "/v1/images/edits" => {
                PriceRequirement::Image(record.image_size.as_deref())
            }
            "/v1/alpha/search" if record.provider_id == codex2api_core::CHATGPT => {
                PriceRequirement::Operation("search")
            }
            "/v1/responses/input_tokens" if record.provider_id == codex2api_core::CHATGPT => {
                PriceRequirement::ReadOnly
            }
            _ => {
                return Err(StorageError::Api {
                    code: "unsupported_endpoint",
                    message: "API 端点不受支持",
                });
            }
        };
        let prices = self.billing_catalog(&record.provider_id).await?;
        let mut tx = self.pool().begin().await?;
        crate::storage::lock_user(&mut tx, &principal.user_id).await?;
        let current = crate::api_access::principal_on(self, &mut tx, &principal.key_id, false)
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
        let prices = self
            .billing_catalog_on(&mut tx, &record.provider_id, prices)
            .await?;
        let kind = if matches!(requirement, PriceRequirement::Image(_)) {
            "image"
        } else {
            "text"
        };
        let supported = prices
            .models
            .iter()
            .any(|m| m.model == model && m.kind == kind && m.enabled && !m.deleted)
            && (current.group.allowed_models.is_empty()
                || current.group.allowed_models.iter().any(|m| m == model));
        if !supported {
            return Err(StorageError::Api {
                code: "model_not_available",
                message: "该 API 组不支持此模型",
            });
        }
        let multiplier = match requirement {
            PriceRequirement::Operation(_) => "1",
            _ => current.group.model_multiplier(model),
        };
        let multiplier_micros = crate::api_groups::multiplier_micros(&multiplier)?;
        let mut operations: Vec<crate::OperationPrice> = Vec::new();
        let mut cost_operations = None;
        let has_price = match requirement {
            PriceRequirement::Tokens(tier) => prices.tokens.iter().any(|p| {
                p.model == model && Some(p.tier.as_str()) == tier && p.min_input_tokens == 0
            }),
            PriceRequirement::Image(size) => {
                let size = size.filter(|size| *size != "auto");
                prices.images.iter().any(|p| {
                    p.model == model
                        && size.is_none_or(|size| {
                            p.resolution == crate::IMAGE_UNIFORM_RESOLUTION
                                || p.resolution == size
                                || (crate::image_resolution_tier(size).is_some()
                                    && crate::image_resolution_tier(&p.resolution).is_some())
                        })
                })
            }
            PriceRequirement::Operation(operation) => {
                cost_operations = Some(
                    prices
                        .operations
                        .iter()
                        .filter(|p| p.operation == operation)
                        .cloned()
                        .collect(),
                );
                operations.push(crate::OperationPrice {
                    provider_id: current.group.provider_id.clone(),
                    operation: operation.into(),
                    price_nano_usd: current
                        .group
                        .search_price_usd
                        .as_deref()
                        .and_then(|price| crate::decimal_units(price, 9)),
                    revision: current.group.revision,
                });
                operations.iter().any(|p| p.price_nano_usd.is_some())
            }
            PriceRequirement::ReadOnly => true,
        };
        if !has_price {
            return Err(match requirement {
                PriceRequirement::Operation(_) => StorageError::Api {
                    code: "operation_price_unavailable",
                    message: "此操作尚未配置按次价格",
                },
                _ => StorageError::Api {
                    code: "model_price_unavailable",
                    message: "此模型、服务档位或图像尺寸尚未配置价格",
                },
            });
        }
        let requires_balance = match requirement {
            PriceRequirement::ReadOnly => false,
            PriceRequirement::Operation(_) => operations.iter().any(|price| {
                price.price_nano_usd.is_some_and(|price| {
                    (i128::from(price) * i128::from(multiplier_micros) + 500_000) / 1_000_000 > 0
                })
            }),
            _ => multiplier_micros != 0,
        };
        if !matches!(requirement, PriceRequirement::ReadOnly) {
            crate::usage_wallet::settle_usage_debt_on(&mut tx, &current.user_id).await?;
            let (balance,pending):(i64,i64)=sqlx::query_as("SELECT u.wallet_cents,COALESCE(s.pending_nano_usd,0) FROM regular_users u LEFT JOIN usage_wallet_state s ON s.user_id=u.id WHERE u.id=$1")
                .bind(&current.user_id).fetch_one(&mut *tx).await?;
            if requires_balance
                && i128::from(balance) * i128::from(NANO_PER_CENT) <= i128::from(pending)
            {
                // Repayment of already delivered usage remains committed even
                // when this request is rejected after the wallet was replenished.
                tx.commit().await?;
                return Err(StorageError::Api {
                    code: "insufficient_balance",
                    message: "钱包余额不足",
                });
            }
        }
        let snapshot = if matches!(requirement, PriceRequirement::Operation(_)) {
            std::sync::Arc::new(serde_json::to_string(&BillingSnapshot::Current {
                tokens: Vec::new(),
                images: Vec::new(),
                operations,
                cost_operations,
            })?)
        } else {
            prices.journal.clone()
        };
        let mut record = record.clone();
        record.status = "in_progress".into();
        record.subject_kind = "api_key".into();
        record.subject_id = current.key_id.clone();
        record.subject_name = sqlx::query_scalar("SELECT name FROM api_keys WHERE id=$1")
            .bind(&current.key_id)
            .fetch_one(&mut *tx)
            .await?;
        record.user_id = Some(current.user_id.clone());
        record.wallet_user_id = Some(current.user_id.clone());
        record.key_id = Some(current.key_id.clone());
        record.group_id = Some(current.group.id.clone());
        record.group_name = Some(current.group.name.clone());
        record.multiplier_micros = multiplier_micros;
        crate::request_writer::insert_usage_on(&mut tx, &record, Some(&snapshot)).await?;
        if self
            .runtime
            .claim_touch(&format!("touch-api-key:{}", current.key_id))
        {
            sqlx::query("UPDATE api_keys SET last_used_at=$1 WHERE id=$2")
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(&current.key_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

use crate::{Result, Storage};
use sqlx::FromRow;

/// The effective supplier pool of a subscription: its explicit override or the plan's tags.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ExecutionRoute {
    pub virtual_account_id: String,
    pub provider_id: String,
    pub tag_ids: Vec<String>,
    pub revision: i64,
}

#[derive(FromRow)]
struct EffectiveRow {
    virtual_account_id: String,
    provider_id: String,
    tag_ids: sqlx::types::Json<Vec<String>>,
    revision: i64,
}

/// The stored route row; `tag_ids` is absent while the subscription inherits its plan.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RouteOverride {
    pub virtual_account_id: String,
    pub provider_id: String,
    pub tag_ids: Option<Vec<String>>,
    pub revision: i64,
}

#[derive(FromRow)]
struct OverrideRow {
    virtual_account_id: String,
    provider_id: String,
    tag_ids: Option<sqlx::types::Json<Vec<String>>>,
    revision: i64,
}

impl Storage {
    pub async fn execution_route_overrides(&self, owner: &str) -> Result<Vec<RouteOverride>> {
        let rows: Vec<OverrideRow> = sqlx::query_as(
            "SELECT virtual_account_id,provider_id,tag_ids,revision FROM execution_routes WHERE virtual_account_id=$1 ORDER BY provider_id",
        )
        .bind(owner)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| RouteOverride {
                virtual_account_id: row.virtual_account_id,
                provider_id: row.provider_id,
                tag_ids: row.tag_ids.map(|tags| tags.0),
                revision: row.revision,
            })
            .collect())
    }

    pub async fn execution_route(
        &self,
        owner: &str,
        provider: &str,
    ) -> Result<Option<ExecutionRoute>> {
        let key = format!("execution-route:{owner}:{provider}");
        self.runtime
            .snapshots
            .read(key.clone(), async {
                let row: Option<EffectiveRow> = sqlx::query_as(
                    "SELECT virtual_account_id,provider_id,tag_ids,revision FROM effective_execution_routes WHERE virtual_account_id=$1 AND provider_id=$2",
                )
                .bind(owner)
                .bind(provider)
                .fetch_optional(self.pool())
                .await?;
                let value = row.map(|row| ExecutionRoute {
                    virtual_account_id: row.virtual_account_id,
                    provider_id: row.provider_id,
                    tag_ids: row.tag_ids.0,
                    revision: row.revision,
                });
                let ttl_ms = if value.is_some() {
                    self.effective_entitlements(owner)
                        .await?
                        .expires_at
                        .map_or(30_000, |at| {
                            at.saturating_mul(1000)
                                .saturating_sub(chrono::Utc::now().timestamp_millis())
                                .clamp(0, 30_000)
                        })
                } else {
                    30_000
                };
                Ok((value, std::time::Duration::from_millis(ttl_ms as u64)))
            })
            .await
    }
}

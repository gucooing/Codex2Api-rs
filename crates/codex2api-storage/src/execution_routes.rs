use crate::{Result, Storage};
use sqlx::FromRow;

#[derive(Clone, Debug, FromRow)]
pub struct ExecutionRoute {
    pub virtual_account_id: String,
    pub provider_id: String,
    pub supplier_account_id: Option<String>,
    pub tag_id: Option<String>,
    pub revision: i64,
}

impl Storage {
    pub async fn execution_routes(&self, owner: &str) -> Result<Vec<ExecutionRoute>> {
        Ok(sqlx::query_as(
            "SELECT * FROM execution_routes WHERE virtual_account_id=? ORDER BY provider_id",
        )
        .bind(owner)
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn execution_route(
        &self,
        owner: &str,
        provider: &str,
    ) -> Result<Option<ExecutionRoute>> {
        Ok(sqlx::query_as(
            "SELECT * FROM execution_routes WHERE virtual_account_id=? AND provider_id=?",
        )
        .bind(owner)
        .bind(provider)
        .fetch_optional(self.pool())
        .await?)
    }

    /// The administrator selects execution supply separately from consumer identity.
    pub async fn save_execution_route(
        &self,
        owner: &str,
        provider: &str,
        supplier: Option<&str>,
        expected: Option<i64>,
    ) -> Result<bool> {
        // Compatibility helper for internal provisioning. The public administrator
        // API always names a tag and cannot bypass pool membership.
        let route = self.execution_route(owner, provider).await?;
        if route.as_ref().map(|r| r.revision) != expected {
            return Ok(false);
        }
        let tag = if let Some(supplier) = supplier {
            let existing = route
                .and_then(|r| r.tag_id)
                .filter(|id| !id.starts_with("legacy-"));
            if let Some(tag) = existing {
                Some(tag)
            } else {
                let tag = format!("legacy-{supplier}");
                self.save_supplier_tag(&tag, provider, &format!("迁移号池 {supplier}"))
                    .await?;
                self.edit_supplier_tags(&[supplier.into()], std::slice::from_ref(&tag), false)
                    .await?;
                Some(tag)
            }
        } else {
            None
        };
        self.save_pool_route(owner, provider, tag.as_deref(), supplier, expected)
            .await
    }
}

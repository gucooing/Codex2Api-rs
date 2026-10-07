use crate::{ExecutionRoute, Result, Storage, StorageError};
use sqlx::{FromRow, SqliteConnection};

#[derive(Clone, Debug, FromRow, serde::Serialize)]
pub struct SupplierTag {
    pub id: String,
    pub provider_id: String,
    pub name: String,
    pub supplier_count: i64,
    pub binding_count: i64,
}

// Selection and assignment run under the same SQLite write transaction. Counting
// every current binding (including other tags) prevents concurrent over-selection.
async fn candidates(conn: &mut SqliteConnection, tag: &str, now: i64) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT a.id FROM supplier_tag_members m
         JOIN supplier_tags t ON t.id=m.tag_id
         JOIN supplier_accounts a ON a.id=m.account_id AND a.provider_id=t.provider_id
         JOIN supplier_tokens k ON k.account_id=a.id
         LEFT JOIN supplier_health h ON h.account_id=a.id
         WHERE m.tag_id=? AND a.status='active' AND COALESCE(k.access_token,'')!=''
           AND (h.rejected_auth_revision IS NULL OR h.rejected_auth_revision!=a.auth_revision)
           AND h.payment_required_at IS NULL
           AND (COALESCE(h.cooldown_kind,'')!='quota_exhausted' OR h.cooldown_auth_revision IS NULL OR h.cooldown_auth_revision!=a.auth_revision OR h.cooldown_until<=?)
         ORDER BY (SELECT COUNT(*) FROM execution_routes r WHERE r.supplier_account_id=a.id),a.created_at,a.id",
    ).bind(tag).bind(now).fetch_all(conn).await?)
}

impl Storage {
    /// A confirmed supplier outage repairs every affected temporary assignment.
    /// Recheck under each assignment transaction so stale health observations or
    /// concurrent administrator changes never force a healthy route elsewhere.
    pub async fn refresh_supplier_bindings(&self, supplier: &str) -> Result<()> {
        let owners: Vec<(String,String)> = sqlx::query_as("SELECT virtual_account_id,provider_id FROM execution_routes WHERE supplier_account_id=? OR (supplier_account_id IS NULL AND tag_id IN (SELECT tag_id FROM supplier_tag_members WHERE account_id=?)) ORDER BY virtual_account_id")
            .bind(supplier).bind(supplier).fetch_all(self.pool()).await?;
        for (owner, provider) in owners {
            self.select_pool_supplier(&owner, &provider, &[]).await?;
        }
        Ok(())
    }
    pub async fn supplier_tags(&self) -> Result<Vec<SupplierTag>> {
        Ok(sqlx::query_as(
            "SELECT t.*,
            (SELECT COUNT(*) FROM supplier_tag_members m WHERE m.tag_id=t.id) AS supplier_count,
            (SELECT COUNT(*) FROM execution_routes r WHERE r.tag_id=t.id) AS binding_count
            FROM supplier_tags t ORDER BY t.provider_id,t.name,t.id",
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn save_supplier_tag(&self, id: &str, provider: &str, name: &str) -> Result<()> {
        if name.trim().is_empty()
            || name.trim().chars().count() > 80
            || name.chars().any(char::is_control)
        {
            return Err(StorageError::Constraint(
                "标签名称须为 1 到 80 个字符".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO supplier_tags(id,provider_id,name) VALUES(?,?,?)
            ON CONFLICT(id) DO UPDATE SET name=excluded.name,provider_id=excluded.provider_id",
        )
        .bind(id)
        .bind(provider)
        .bind(name.trim())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_supplier_tag(&self, id: &str) -> Result<()> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let bound: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_routes WHERE tag_id=?) OR EXISTS(SELECT 1 FROM virtual_plans WHERE json_extract(config,'$.supplier_tag_id')=?)")
                .bind(id)
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if bound {
            return Err(StorageError::Constraint(
                "此标签仍绑定虚拟账户，请先更换其号池".into(),
            ));
        }
        sqlx::query("DELETE FROM supplier_tags WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn supplier_tag_ids(&self, account: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT tag_id FROM supplier_tag_members WHERE account_id=? ORDER BY tag_id",
        )
        .bind(account)
        .fetch_all(self.pool())
        .await?)
    }

    /// Replace the selected accounts' complete tag sets atomically. An empty
    /// submitted set clears tags; callers must not submit unloaded placeholders.
    pub async fn replace_supplier_tags(&self, accounts: &[String], tags: &[String]) -> Result<()> {
        let accounts: std::collections::BTreeSet<_> = accounts.iter().collect();
        let tags: std::collections::BTreeSet<_> = tags.iter().collect();
        if accounts.is_empty() || accounts.len() > 1000 || tags.len() > 100 {
            return Err(StorageError::Constraint(
                "请选择 1 到 1000 个账户，标签不能超过 100 个".into(),
            ));
        }
        let account_json = serde_json::to_string(&accounts)?;
        let tag_json = serde_json::to_string(&tags)?;
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let found: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM supplier_accounts WHERE id IN(SELECT value FROM json_each(?))",
        )
        .bind(&account_json)
        .fetch_one(&mut *tx)
        .await?;
        if found != accounts.len() as i64 {
            return Err(StorageError::Constraint(
                "所选供应账户已变更，请刷新后重试".into(),
            ));
        }
        let providers:Vec<String>=sqlx::query_scalar("SELECT DISTINCT provider_id FROM supplier_accounts WHERE id IN(SELECT value FROM json_each(?))")
            .bind(&account_json).fetch_all(&mut *tx).await?;
        for provider in providers {
            let valid:i64=sqlx::query_scalar("SELECT COUNT(*) FROM supplier_tags WHERE provider_id=? AND id IN(SELECT value FROM json_each(?))")
                .bind(provider).bind(&tag_json).fetch_one(&mut *tx).await?;
            if valid != tags.len() as i64 {
                return Err(StorageError::Constraint(
                    "只能选择与供应账户同平台的标签".into(),
                ));
            }
        }
        let routes:Vec<(String,String)>=sqlx::query_as("SELECT virtual_account_id,provider_id FROM execution_routes WHERE supplier_account_id IN(SELECT value FROM json_each(?))")
            .bind(&account_json).fetch_all(&mut *tx).await?;
        sqlx::query("DELETE FROM supplier_tag_members WHERE account_id IN(SELECT value FROM json_each(?)) AND tag_id NOT IN(SELECT value FROM json_each(?))")
            .bind(&account_json).bind(&tag_json).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO supplier_tag_members(tag_id,account_id) SELECT t.value,a.value FROM json_each(?) t CROSS JOIN json_each(?) a WHERE 1 ON CONFLICT(tag_id,account_id) DO NOTHING")
            .bind(&tag_json).bind(&account_json).execute(&mut *tx).await?;
        tx.commit().await?;
        for (owner, provider) in routes {
            self.select_pool_supplier(&owner, &provider, &[]).await?;
        }
        Ok(())
    }

    /// Batch add/remove is atomic and never replaces unrelated tags.
    pub async fn edit_supplier_tags(
        &self,
        accounts: &[String],
        tags: &[String],
        remove: bool,
    ) -> Result<()> {
        if accounts.is_empty() || accounts.len() > 1000 || tags.is_empty() || tags.len() > 100 {
            return Err(StorageError::Constraint(
                "请选择 1 到 1000 个账户和 1 到 100 个标签".into(),
            ));
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        for account in accounts {
            for tag in tags {
                let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM supplier_accounts a JOIN supplier_tags t ON t.provider_id=a.provider_id WHERE a.id=? AND t.id=?)")
                    .bind(account).bind(tag).fetch_one(&mut *tx).await?;
                if !valid {
                    return Err(StorageError::Constraint(
                        "标签与供应账户必须属于同一提供商".into(),
                    ));
                }
                let query = if remove {
                    "DELETE FROM supplier_tag_members WHERE tag_id=? AND account_id=?"
                } else {
                    "INSERT INTO supplier_tag_members(tag_id,account_id) VALUES(?,?) ON CONFLICT DO NOTHING"
                };
                sqlx::query(query)
                    .bind(tag)
                    .bind(account)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn supplier_binding_count(&self, account: &str) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM execution_routes WHERE supplier_account_id=?")
                .bind(account)
                .fetch_one(self.pool())
                .await?,
        )
    }

    pub async fn save_pool_route(
        &self,
        owner: &str,
        provider: &str,
        tag: Option<&str>,
        supplier: Option<&str>,
        expected: Option<i64>,
    ) -> Result<bool> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let current: Option<ExecutionRoute> = sqlx::query_as(
            "SELECT * FROM execution_routes WHERE virtual_account_id=? AND provider_id=?",
        )
        .bind(owner)
        .bind(provider)
        .fetch_optional(&mut *tx)
        .await?;
        if current.as_ref().map(|r| r.revision) != expected {
            return Ok(false);
        }
        let selected = if let Some(tag) = tag {
            let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM supplier_tags t JOIN virtual_accounts v ON v.provider_id=t.provider_id WHERE t.id=? AND v.id=? AND t.provider_id=?)")
                .bind(tag).bind(owner).bind(provider).fetch_one(&mut *tx).await?;
            if !valid {
                return Err(StorageError::Constraint("请选择同提供商的标签号池".into()));
            }
            let available = candidates(&mut tx, tag, chrono::Utc::now().timestamp()).await?;
            if let Some(supplier) = supplier {
                if !available.iter().any(|id| id == supplier) {
                    return Err(StorageError::Constraint(
                        "只能绑定所选标签号池内状态正常的供应账户".into(),
                    ));
                }
                Some(supplier.to_owned())
            } else {
                available.into_iter().next()
            }
        } else {
            if supplier.is_some() {
                return Err(StorageError::Constraint("请先选择标签号池".into()));
            }
            None
        };
        sqlx::query("INSERT INTO execution_routes(virtual_account_id,provider_id,tag_id,supplier_account_id,revision) VALUES(?,?,?,?,?)
            ON CONFLICT(virtual_account_id,provider_id) DO UPDATE SET tag_id=excluded.tag_id,supplier_account_id=excluded.supplier_account_id,revision=excluded.revision")
            .bind(owner).bind(provider).bind(tag).bind(selected).bind(expected.unwrap_or(0)+1).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Healthy assignments are sticky. An unavailable/missing assignment is
    /// replaced using the least-bound member. Exclusions bound a request's retries.
    pub async fn select_pool_supplier(
        &self,
        owner: &str,
        provider: &str,
        excluded: &[String],
    ) -> Result<Option<String>> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        // User platforms inherit the current plan's pool. An expired paid plan
        // uses the platform Free pool, so paid supply is not an expired benefit.
        let inherited:Option<(Option<String>,bool)>=sqlx::query_as("SELECT json_extract(p.config,'$.supplier_tag_id'),(v.plan_type!='free' AND v.subscription_expires_at IS NOT NULL AND unixepoch(v.subscription_expires_at)<=unixepoch()) FROM user_subscriptions s JOIN virtual_accounts v ON v.id=s.virtual_account_id JOIN platform_free_plans f ON f.provider_id=s.provider_id JOIN virtual_plans p ON p.id=CASE WHEN v.plan_type!='free' AND v.subscription_expires_at IS NOT NULL AND unixepoch(v.subscription_expires_at)<=unixepoch() THEN f.plan_id ELSE v.plan_id END WHERE v.id=? AND v.provider_id=?")
            .bind(owner).bind(provider).fetch_optional(&mut *tx).await?;
        if let Some((tag, expired)) = inherited {
            sqlx::query("INSERT INTO execution_routes(virtual_account_id,provider_id,tag_id,supplier_account_id) VALUES(?,?,?,NULL) ON CONFLICT(virtual_account_id,provider_id) DO UPDATE SET tag_id=excluded.tag_id,supplier_account_id=NULL,revision=revision+1 WHERE (? OR execution_routes.tag_id IS NULL) AND execution_routes.tag_id IS NOT excluded.tag_id")
                .bind(owner).bind(provider).bind(tag).bind(expired).execute(&mut *tx).await?;
        }
        let route: Option<ExecutionRoute> = sqlx::query_as("SELECT r.* FROM execution_routes r JOIN virtual_principals v ON v.id=r.virtual_account_id WHERE r.virtual_account_id=? AND r.provider_id=? AND v.enabled=1")
            .bind(owner).bind(provider).fetch_optional(&mut *tx).await?;
        let Some(route) = route else {
            return Ok(None);
        };
        let Some(tag) = &route.tag_id else {
            return Ok(None);
        };
        let available = candidates(&mut tx, tag, chrono::Utc::now().timestamp()).await?;
        let eligible = |id: &String| !excluded.contains(id);
        let selected = route
            .supplier_account_id
            .as_ref()
            .filter(|id| available.contains(id) && eligible(id))
            .cloned()
            .or_else(|| available.into_iter().find(eligible));
        if selected != route.supplier_account_id {
            sqlx::query("UPDATE execution_routes SET supplier_account_id=?,revision=revision+1 WHERE virtual_account_id=? AND provider_id=?")
                .bind(&selected).bind(owner).bind(provider).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(selected)
    }
}

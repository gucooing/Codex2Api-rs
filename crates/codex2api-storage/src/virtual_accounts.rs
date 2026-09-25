use crate::{Result, Storage, StorageError, hash_token};
use chrono::Utc;
use sqlx::FromRow;

#[derive(Clone, FromRow)]
pub struct VirtualAccount {
    pub provider_id: String,
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub name: String,
    pub email: String,
    pub plan_type: String,
    pub plan_id: String,
    pub subscription_expires_at: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}
impl VirtualAccount {
    pub fn effective_plan_at(&self, now: i64) -> &str {
        if self.plan_type != "free"
            && self
                .subscription_expires_at
                .as_deref()
                .is_none_or(|expiry| {
                    chrono::DateTime::parse_from_rfc3339(expiry)
                        .is_ok_and(|time| time.timestamp() > now)
                })
        {
            &self.plan_type
        } else {
            "free"
        }
    }
    pub fn effective_plan(&self) -> &str {
        self.effective_plan_at(Utc::now().timestamp())
    }
}
#[derive(Clone, FromRow, serde::Serialize)]
pub struct VirtualDevice {
    pub provider_id: String,
    pub scopes: String,
    pub id: String,
    pub virtual_account_id: String,
    pub installation_id: Option<String>,
    pub user_agent: String,
    pub created_at: String,
    pub last_login_at: String,
    pub last_used_at: Option<String>,
}
#[derive(Clone, FromRow)]
pub struct VirtualAccess {
    pub scopes: String,
    pub provider_id: String,
    pub virtual_account_id: String,
    pub device_id: String,
    pub account_id: Option<String>,
    pub name: String,
    pub token_hash: String,
}
#[derive(FromRow, serde::Serialize)]
pub struct MissingEndpoint {
    pub method: String,
    pub path: String,
    pub hits: i64,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub last_success_at: Option<String>,
}

impl Storage {
    pub async fn allow_virtual_login_attempt(&self, username: &str) -> Result<bool> {
        let now = Utc::now().timestamp();
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM virtual_login_attempts WHERE window_start<?")
            .bind(now - 60)
            .execute(&mut *tx)
            .await?;
        let attempts:i64=sqlx::query_scalar("INSERT INTO virtual_login_attempts(username_hash,attempts,window_start) VALUES(?,1,?) ON CONFLICT(username_hash) DO UPDATE SET attempts=attempts+1 RETURNING attempts")
            .bind(hash_token(&username.to_lowercase())).bind(now).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(attempts <= 10)
    }
    pub async fn virtual_accounts(&self) -> Result<Vec<VirtualAccount>> {
        Ok(
            sqlx::query_as("SELECT * FROM virtual_accounts ORDER BY created_at,id")
                .fetch_all(self.pool())
                .await?,
        )
    }

    /// Resolve a filter in SQLite so administrative bulk operations never need
    /// to materialize every matching account in the browser.
    pub async fn virtual_account_ids_matching(
        &self,
        search: &str,
        enabled: Option<bool>,
        subscription: Option<&str>,
    ) -> Result<Vec<String>> {
        let now = Utc::now().timestamp();
        let mut query = String::from("SELECT id FROM virtual_accounts WHERE 1=1");
        let mut binds: Vec<String> = Vec::new();
        if !search.trim().is_empty() {
            query.push_str(" AND (instr(lower(username),lower(?))>0 OR instr(lower(name),lower(?))>0 OR instr(lower(email),lower(?))>0)");
            binds.extend([
                search.trim().to_owned(),
                search.trim().to_owned(),
                search.trim().to_owned(),
            ]);
        }
        if let Some(enabled) = enabled {
            query.push_str(if enabled {
                " AND enabled=1"
            } else {
                " AND enabled=0"
            });
        }
        match subscription {
            Some("free") => query.push_str(" AND plan_type='free'"),
            Some("active") => query.push_str(" AND plan_type!='free' AND (subscription_expires_at IS NULL OR unixepoch(subscription_expires_at)>?)"),
            Some("expired") => query.push_str(" AND plan_type!='free' AND subscription_expires_at IS NOT NULL AND unixepoch(subscription_expires_at)<=?"),
            Some("") | None => {}
            Some(_) => return Err(StorageError::Constraint("订阅筛选条件无效".into())),
        }
        let mut q = sqlx::query_scalar::<_, String>(&query);
        for bind in binds {
            q = q.bind(bind);
        }
        if matches!(subscription, Some("active") | Some("expired")) {
            q = q.bind(now);
        }
        Ok(q.fetch_all(self.pool()).await?)
    }
    pub async fn virtual_account(&self, id: &str) -> Result<Option<VirtualAccount>> {
        Ok(sqlx::query_as("SELECT * FROM virtual_accounts WHERE id=?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }
    pub async fn search_virtual_accounts(
        &self,
        search: &str,
        limit: u32,
    ) -> Result<Vec<VirtualAccount>> {
        Ok(sqlx::query_as(
            "SELECT * FROM virtual_accounts
             WHERE instr(lower(username), lower(?)) > 0 OR instr(lower(email), lower(?)) > 0
             ORDER BY created_at, id LIMIT ?",
        )
        .bind(search)
        .bind(search)
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await?)
    }
    pub async fn virtual_account_by_username(
        &self,
        username: &str,
    ) -> Result<Option<VirtualAccount>> {
        Ok(sqlx::query_as(
            "SELECT * FROM virtual_accounts WHERE username=? COLLATE NOCASE AND enabled=1",
        )
        .bind(username)
        .fetch_optional(self.pool())
        .await?)
    }
    pub async fn save_virtual_account(&self, account: &VirtualAccount) -> Result<()> {
        self.save_virtual_account_operation(account, "system").await
    }
    pub async fn save_virtual_account_operation(
        &self,
        account: &VirtualAccount,
        origin: &str,
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        // A write first avoids read-to-write upgrade races with concurrent logins.
        sqlx::query("DELETE FROM virtual_devices WHERE virtual_account_id=? AND EXISTS(SELECT 1 FROM virtual_accounts WHERE id=? AND (password_hash!=? OR ?=0))")
            .bind(&account.id).bind(&account.id).bind(&account.password_hash).bind(account.enabled).execute(&mut *tx).await?;
        let previous: Option<VirtualAccount> =
            sqlx::query_as("SELECT * FROM virtual_accounts WHERE id=?")
                .bind(&account.id)
                .fetch_optional(&mut *tx)
                .await?;
        let selectable: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM virtual_plans WHERE id=? AND plan_type=? AND (enabled=1 OR id=?))")
            .bind(&account.plan_id).bind(&account.plan_type).bind(previous.as_ref().map(|a| &a.plan_id)).fetch_one(&mut *tx).await?;
        if !selectable {
            return Err(StorageError::Constraint(
                "请选择套餐管理中可用的套餐".into(),
            ));
        }
        sqlx::query("INSERT INTO virtual_accounts(provider_id,id,username,password_hash,name,email,plan_type,subscription_expires_at,enabled,created_at,plan_id) VALUES(?,?,?,?,?,?,?,?,?,?,?)
            ON CONFLICT(id) DO UPDATE SET username=excluded.username,password_hash=excluded.password_hash,name=excluded.name,email=excluded.email,plan_type=excluded.plan_type,plan_id=excluded.plan_id,subscription_expires_at=excluded.subscription_expires_at,enabled=excluded.enabled")
            .bind(&account.provider_id).bind(&account.id).bind(&account.username).bind(&account.password_hash).bind(&account.name).bind(&account.email).bind(&account.plan_type).bind(&account.subscription_expires_at)
            .bind(account.enabled).bind(&account.created_at).bind(&account.plan_id).execute(&mut *tx).await?;
        // 首次发放、换套餐或过期后重新发放才重启顶层；有效期内续期保留周期。
        let now = Utc::now();
        let restart = previous.as_ref().is_none_or(|old| {
            old.plan_id != account.plan_id
                || old.plan_type != account.plan_type
                || (old.effective_plan_at(now.timestamp()) == "free"
                    && account.effective_plan_at(now.timestamp()) != "free")
        });
        if restart {
            let started_at = if previous.is_none() {
                account.created_at.clone()
            } else {
                now.to_rfc3339()
            };
            sqlx::query("UPDATE virtual_accounts SET subscription_started_at=? WHERE id=?")
                .bind(started_at)
                .bind(&account.id)
                .execute(&mut *tx)
                .await?;
        }
        if previous.as_ref().is_none_or(|old| {
            old.plan_id != account.plan_id
                || old.plan_type != account.plan_type
                || old.subscription_expires_at != account.subscription_expires_at
        }) {
            let operation = match previous.as_ref() {
                None => "grant",
                Some(old)
                    if old.plan_id != account.plan_id || old.plan_type != account.plan_type =>
                {
                    "change_plan"
                }
                Some(old)
                    if account.subscription_expires_at.is_none()
                        || old
                            .subscription_expires_at
                            .as_deref()
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .zip(
                                account
                                    .subscription_expires_at
                                    .as_deref()
                                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()),
                            )
                            .is_some_and(|(old, new)| new > old) =>
                {
                    "renew"
                }
                Some(_) => "change_expiry",
            };
            let now = Utc::now().timestamp_millis();
            let plan_name: String = sqlx::query_scalar("SELECT name FROM virtual_plans WHERE id=?")
                .bind(&account.plan_id)
                .fetch_one(&mut *tx)
                .await?;
            let previous_name: Option<String> =
                sqlx::query_scalar("SELECT name FROM virtual_plans WHERE id=?")
                    .bind(previous.as_ref().map(|a| &a.plan_id))
                    .fetch_optional(&mut *tx)
                    .await?;
            let value = serde_json::json!({"operation":operation,"origin":origin,"plan_type":account.plan_type,"plan_id":account.plan_id,"plan_name":plan_name,"previous_plan_name":previous_name,"previous_plan_id":previous.as_ref().map(|a| &a.plan_id),"expires_at":account.subscription_expires_at,"previous_plan":previous.as_ref().map(|a| &a.plan_type),"previous_expires_at":previous.as_ref().and_then(|a| a.subscription_expires_at.as_ref()),"created_at_ms":now});
            sqlx::query("INSERT INTO virtual_resources(virtual_account_id,kind,id,value_json,created_at_ms,updated_at_ms) VALUES(?,'subscription_operation',?,?,?,?)")
                .bind(&account.id).bind(uuid::Uuid::new_v4().to_string()).bind(value.to_string()).bind(now).bind(now).execute(&mut *tx).await?;
        }
        sqlx::query("DELETE FROM virtual_authorization_codes WHERE virtual_account_id=?")
            .bind(&account.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn delete_virtual_account(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM virtual_accounts WHERE id=?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn virtual_devices(&self, id: &str) -> Result<Vec<VirtualDevice>> {
        Ok(sqlx::query_as("SELECT provider_id,scopes,id,virtual_account_id,installation_id,user_agent,created_at,last_login_at,last_used_at FROM virtual_devices WHERE virtual_account_id=? ORDER BY created_at DESC")
            .bind(id).fetch_all(self.pool()).await?)
    }
    pub async fn revoke_virtual_device(&self, owner: &str, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM virtual_devices WHERE virtual_account_id=? AND id=?")
            .bind(owner)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn virtual_refresh_device(&self, token: &str) -> Result<Option<VirtualDevice>> {
        Ok(sqlx::query_as("SELECT d.provider_id,d.scopes,d.id,d.virtual_account_id,d.installation_id,d.user_agent,d.created_at,d.last_login_at,d.last_used_at FROM virtual_devices d JOIN virtual_accounts v ON v.id=d.virtual_account_id WHERE d.refresh_hash=? AND v.enabled=1")
            .bind(hash_token(token)).fetch_optional(self.pool()).await?)
    }
    pub async fn create_virtual_device(
        &self,
        owner: &VirtualAccount,
        token: &str,
        device: &crate::OAuthDeviceIdentity,
    ) -> Result<Option<String>> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let changed=sqlx::query("INSERT INTO virtual_devices(id,virtual_account_id,refresh_hash,installation_id,user_agent,created_at,last_login_at,provider_id)
            SELECT ?,v.id,?,?,?,?,?,v.provider_id FROM virtual_accounts v WHERE v.id=? AND v.enabled=1 AND v.password_hash=?")
            .bind(&id).bind(hash_token(token)).bind(&device.installation_id).bind(&device.user_agent).bind(&now).bind(&now).bind(&owner.id).bind(&owner.password_hash).execute(self.pool()).await?.rows_affected();
        Ok((changed == 1).then_some(id))
    }
    pub async fn register_virtual_access(
        &self,
        device: &str,
        refresh: &str,
        token: &str,
        expires: i64,
    ) -> Result<bool> {
        self.register_virtual_access_scoped(device, refresh, token, expires, None)
            .await
    }

    pub async fn register_virtual_access_scoped(
        &self,
        device: &str,
        refresh: &str,
        token: &str,
        expires: i64,
        requested_scopes: Option<&str>,
    ) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM virtual_access_tokens WHERE expires_at<=?")
            .bind(Utc::now().timestamp())
            .execute(&mut *tx)
            .await?;
        let grant: Option<String> = sqlx::query_scalar("SELECT d.scopes FROM virtual_devices d JOIN virtual_accounts v ON v.id=d.virtual_account_id WHERE d.id=? AND d.refresh_hash=? AND v.enabled=1")
            .bind(device).bind(hash_token(refresh)).fetch_optional(&mut *tx).await?;
        let Some(grant) = grant else {
            return Ok(false);
        };
        let scopes = requested_scopes.unwrap_or(&grant);
        if !scopes
            .split_whitespace()
            .all(|scope| grant.split_whitespace().any(|allowed| allowed == scope))
        {
            return Ok(false);
        }
        let changed=sqlx::query("INSERT INTO virtual_access_tokens(token_hash,device_id,expires_at,scopes) SELECT ?,d.id,?,? FROM virtual_devices d JOIN virtual_accounts v ON v.id=d.virtual_account_id WHERE d.id=? AND d.refresh_hash=? AND v.enabled=1")
            .bind(hash_token(token)).bind(expires).bind(scopes).bind(device).bind(hash_token(refresh)).execute(&mut *tx).await?.rows_affected();
        sqlx::query("UPDATE virtual_devices SET last_login_at=? WHERE id=?")
            .bind(Utc::now().to_rfc3339())
            .bind(device)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(changed == 1)
    }
    pub async fn virtual_access(&self, hash: &str) -> Result<Option<VirtualAccess>> {
        Ok(sqlx::query_as("SELECT t.scopes,d.provider_id,v.id AS virtual_account_id,d.id AS device_id,r.supplier_account_id AS account_id,v.name,t.token_hash FROM virtual_access_tokens t JOIN virtual_devices d ON d.id=t.device_id JOIN virtual_accounts v ON v.id=d.virtual_account_id LEFT JOIN execution_routes r ON r.virtual_account_id=v.id AND r.provider_id=d.provider_id WHERE t.token_hash=? AND t.expires_at>? AND v.enabled=1")
            .bind(hash).bind(Utc::now().timestamp()).fetch_optional(self.pool()).await?)
    }
    pub async fn touch_virtual_access(&self, hash: &str) -> Result<()> {
        sqlx::query("UPDATE virtual_devices SET last_used_at=? WHERE id IN(SELECT device_id FROM virtual_access_tokens WHERE token_hash=? AND expires_at>?)")
            .bind(Utc::now().to_rfc3339()).bind(hash).bind(Utc::now().timestamp()).execute(self.pool()).await?;
        Ok(())
    }
    pub async fn revoke_virtual_token(&self, token: &str, provider: &str) -> Result<()> {
        let hash = hash_token(token);
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM virtual_devices WHERE refresh_hash=? AND provider_id=?")
            .bind(&hash)
            .bind(provider)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM virtual_access_tokens WHERE token_hash=? AND device_id IN(SELECT id FROM virtual_devices WHERE provider_id=?)")
            .bind(hash).bind(provider)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn virtual_usage_tokens(&self, id: &str, from: i64) -> Result<i64> {
        Ok(sqlx::query_scalar("SELECT COALESCE(SUM(COALESCE(input_tokens,0)+COALESCE(output_tokens,0)),0) FROM usage_records WHERE subject_id=? AND requested_at_ms>=?")
            .bind(id).bind(from*1000).fetch_one(self.pool()).await?)
    }
    pub async fn record_missing_endpoint(&self, method: &str, path: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO oauth_missing_endpoints(method,path,hits,first_seen_at,last_seen_at) VALUES(?,?,1,?,?) ON CONFLICT(method,path) DO UPDATE SET hits=hits+1,last_seen_at=excluded.last_seen_at")
            .bind(method).bind(path).bind(&now).bind(&now).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM oauth_missing_endpoints WHERE rowid IN(SELECT rowid FROM oauth_missing_endpoints ORDER BY last_seen_at DESC LIMIT -1 OFFSET 500)").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn missing_endpoints(&self) -> Result<Vec<MissingEndpoint>> {
        Ok(
            sqlx::query_as("SELECT m.*,strftime('%Y-%m-%dT%H:%M:%f+00:00',s.at/1000.0,'unixepoch') AS last_success_at FROM oauth_missing_endpoints m LEFT JOIN (
                SELECT method,path,MAX(at) AS at FROM (
                    SELECT method,path,created_at_ms AS at FROM virtual_request_logs WHERE status BETWEEN 200 AND 299
                    UNION ALL SELECT 'POST',CASE source WHEN 'telemetry' THEN '/api/oauth/chatgpt/ces/v1/telemetry/intake' WHEN 'statsig_events' THEN '/api/oauth/chatgpt/ces/v1/rgstr' WHEN 'sdk_exception' THEN '/api/oauth/chatgpt/v1/sdk_exception' END,last_seen_at_ms FROM desktop_diagnostics
                    UNION ALL SELECT 'GET','/api/oauth/chatgpt'||path,fetched_at_ms FROM desktop_public_resources
                ) GROUP BY method,path
            ) s ON s.method=m.method AND s.path=m.path ORDER BY m.last_seen_at DESC")
                .fetch_all(self.pool())
                .await?,
        )
    }
}

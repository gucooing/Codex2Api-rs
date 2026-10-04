//! User identity and browser sessions. These records never contain supplier data.
use crate::{Result, Storage, StorageError, VirtualAccount, hash_token};
use chrono::Utc;
use serde::Serialize;
use sqlx::FromRow;

#[derive(Clone, FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub name: String,
    pub email: String,
    pub enabled: bool,
    pub wallet_cents: i64,
    pub revision: i64,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct UserView {
    pub id: String,
    pub username: String,
    pub name: String,
    pub email: String,
    pub enabled: bool,
    pub wallet_balance_usd: String,
    pub revision: i64,
    pub created_at: String,
}
impl User {
    pub fn view(&self) -> UserView {
        UserView {
            id: self.id.clone(),
            username: self.username.clone(),
            name: self.name.clone(),
            email: self.email.clone(),
            enabled: self.enabled,
            wallet_balance_usd: crate::format_units(self.wallet_cents, 2),
            revision: self.revision,
            created_at: self.created_at.clone(),
        }
    }
}
#[derive(Clone, FromRow)]
pub struct UserSession {
    pub user_id: String,
    pub token_hash: String,
    pub csrf_token: String,
    pub expires_at: i64,
}

impl Storage {
    pub async fn admin_overview(&self, tz_offset: i32) -> Result<serde_json::Value> {
        if !(-840..=840).contains(&tz_offset) {
            return Err(StorageError::InvalidAdminUpdate("时区无效"));
        }
        let now = Utc::now().timestamp_millis();
        let zone = i64::from(tz_offset) * 60_000;
        let start = (now - zone).div_euclid(86_400_000) * 86_400_000 + zone;
        let (suppliers,consumers,normal,users,active,models):(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT COUNT(*) FROM supplier_accounts),(SELECT COUNT(*) FROM virtual_accounts v WHERE NOT EXISTS(SELECT 1 FROM user_subscriptions s WHERE s.virtual_account_id=v.id)),(SELECT COUNT(*) FROM virtual_accounts v WHERE v.enabled=1 AND (v.subscription_expires_at IS NULL OR unixepoch(v.subscription_expires_at)>?) AND NOT EXISTS(SELECT 1 FROM user_subscriptions s WHERE s.virtual_account_id=v.id)),(SELECT COUNT(*) FROM users),(SELECT COUNT(DISTINCT s.user_id) FROM user_subscriptions s JOIN usage_records r ON r.subject_id=s.virtual_account_id AND r.subject_kind='virtual_account' WHERE r.requested_at_ms>=? AND r.requested_at_ms<?),(SELECT COUNT(*) FROM model_catalog WHERE provider_id='chatgpt' AND deleted=0)")
            .bind(now/1000).bind(start).bind(now+1).fetch_one(self.pool()).await?;
        Ok(
            serde_json::json!({"supplier_count":suppliers,"consumer_count":consumers,"normal_consumer_count":normal,"user_count":users,"active_user_count":active,"models_count":models,"activity_from_ms":start,"activity_until_ms":now+1}),
        )
    }
    pub async fn user_options(&self, search: &str) -> Result<serde_json::Value> {
        if search.len() > 128 {
            return Err(StorageError::InvalidAdminUpdate("用户搜索最多 128 字节"));
        }
        let rows:Vec<(String,String,String)> = sqlx::query_as("SELECT id,username,name FROM user_identities WHERE instr(lower(username),lower(?))>0 OR instr(lower(name),lower(?))>0 OR instr(lower(email),lower(?))>0 ORDER BY username,id LIMIT 5")
            .bind(search.trim()).bind(search.trim()).bind(search.trim()).fetch_all(self.pool()).await?;
        Ok(
            serde_json::json!({"items":rows.into_iter().map(|(id,username,name)|serde_json::json!({"id":id,"username":username,"name":name})).collect::<Vec<_>>()}),
        )
    }
    pub async fn clear_browser_identity(&self, flow: &str) -> Result<()> {
        sqlx::query("DELETE FROM oauth_browser_identities WHERE flow_id=?")
            .bind(flow)
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn users(&self) -> Result<Vec<User>> {
        Ok(
            sqlx::query_as("SELECT * FROM user_identities ORDER BY created_at DESC,id")
                .fetch_all(self.pool())
                .await?,
        )
    }
    pub async fn user(&self, id: &str) -> Result<Option<User>> {
        Ok(sqlx::query_as("SELECT * FROM user_identities WHERE id=?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }
    pub async fn user_by_username(&self, username: &str) -> Result<Option<User>> {
        Ok(sqlx::query_as(
            "SELECT * FROM user_identities WHERE username=? COLLATE NOCASE AND enabled=1",
        )
        .bind(username)
        .fetch_optional(self.pool())
        .await?)
    }
    pub async fn save_user(&self, user: &User, expected: Option<i64>) -> Result<bool> {
        if user.username.trim().is_empty()
            || user.username.len() > 128
            || user.name.trim().is_empty()
            || user.name.len() > 128
            || user.email.len() > 254
            || !user.email.contains('@')
        {
            return Err(StorageError::InvalidAdminUpdate(
                "请填写有效的用户名、名称和邮箱",
            ));
        }
        // Wallet amounts cannot be set through identity edits, including account creation.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let now = Utc::now().to_rfc3339();
        let changed = if let Some(revision) = expected {
            let changed=sqlx::query("UPDATE users SET name=?,email=?,revision=revision+1 WHERE id=? AND revision=? AND EXISTS(SELECT 1 FROM accounts WHERE accounts.id=users.id AND account_type='user')")
                .bind(user.name.trim()).bind(user.email.trim()).bind(&user.id).bind(revision).execute(&mut *tx).await?.rows_affected();
            if changed == 1 {
                let account=sqlx::query("UPDATE accounts SET username=?,password_hash=?,enabled=?,updated_at=? WHERE id=? AND account_type='user'")
                    .bind(user.username.trim()).bind(&user.password_hash).bind(user.enabled).bind(&now).bind(&user.id).execute(&mut *tx).await?.rows_affected();
                if account != 1 {
                    return Err(StorageError::InvalidCredentials);
                }
            }
            changed
        } else {
            sqlx::query("INSERT INTO accounts(id,account_type,username,password_hash,enabled,created_at,updated_at) VALUES(?,'user',?,?,?,?,?)")
                .bind(&user.id).bind(user.username.trim()).bind(&user.password_hash).bind(user.enabled).bind(&user.created_at).bind(&now).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO users(id,name,email) VALUES(?,?,?)")
                .bind(&user.id)
                .bind(user.name.trim())
                .bind(user.email.trim())
                .execute(&mut *tx)
                .await?
                .rows_affected()
        };
        if expected.is_none() && changed == 1 {
            let defaults: Vec<crate::VirtualPlan> = sqlx::query_as(
                "SELECT p.* FROM virtual_plans p JOIN platform_free_plans f ON f.plan_id=p.id",
            )
            .fetch_all(&mut *tx)
            .await?;
            for plan in defaults {
                crate::user_subscriptions::write_subscription(
                    &mut tx, user, &plan, None, None, true, "free",
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(changed == 1)
    }
    pub async fn create_user_session(&self, user: &User, csrf: &str) -> Result<Option<String>> {
        let now = Utc::now().timestamp();
        let expires = now + 86400;
        let token = self
            .sign_jwt(
                crate::TokenPurpose::UserSession,
                serde_json::json!({
                    "sub":user.id,"jti":uuid::Uuid::new_v4().to_string(),"iat":now,"exp":expires
                }),
            )
            .await?;
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("DELETE FROM user_sessions WHERE expires_at<=?")
            .bind(Utc::now().timestamp())
            .execute(&mut *tx)
            .await?;
        let inserted=sqlx::query("INSERT INTO user_sessions(token_hash,user_id,csrf_token,expires_at) SELECT ?,id,?,? FROM user_identities WHERE id=? AND enabled=1 AND password_hash=?")
            .bind(hash_token(&token)).bind(csrf).bind(expires)
            .bind(&user.id).bind(&user.password_hash).execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;
        Ok((inserted == 1).then_some(token))
    }
    pub async fn user_session(&self, token: &str) -> Result<Option<UserSession>> {
        let claims = match self
            .verify_jwt(crate::TokenPurpose::UserSession, token)
            .await
        {
            Ok(claims) => claims,
            Err(StorageError::InvalidJwt) => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(sqlx::query_as("SELECT s.* FROM user_sessions s JOIN user_identities u ON u.id=s.user_id WHERE s.token_hash=? AND s.expires_at>? AND u.enabled=1 AND u.id=?")
            .bind(hash_token(token)).bind(Utc::now().timestamp()).bind(claims["sub"].as_str()).fetch_optional(self.pool()).await?)
    }
    pub async fn revoke_user_session(&self, hash: &str) -> Result<()> {
        sqlx::query("DELETE FROM user_sessions WHERE token_hash=?")
            .bind(hash)
            .execute(self.pool())
            .await?;
        Ok(())
    }
    pub async fn user_platform_account(
        &self,
        user: &str,
        provider: &str,
    ) -> Result<Option<VirtualAccount>> {
        let id:Option<String>=sqlx::query_scalar("SELECT v.id FROM virtual_principals v JOIN user_subscriptions s ON s.virtual_account_id=v.id JOIN user_identities u ON u.id=s.user_id WHERE u.id=? AND s.provider_id=? AND u.enabled=1 AND v.enabled=1")
            .bind(user).bind(provider).fetch_optional(self.pool()).await?;
        match id {
            Some(id) => self.effective_virtual_account(&id).await,
            None => Ok(None),
        }
    }

    pub async fn virtual_account_user(&self, account: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT user_id FROM user_subscriptions WHERE virtual_account_id=?")
                .bind(account)
                .fetch_optional(self.pool())
                .await?,
        )
    }
    pub async fn virtual_owner_enabled(&self, account: &str) -> Result<bool> {
        Ok(sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM user_subscriptions s JOIN user_identities u ON u.id=s.user_id WHERE s.virtual_account_id=? AND u.enabled=0)")
            .bind(account).fetch_one(self.pool()).await?)
    }
    pub async fn bind_browser_identity(
        &self,
        flow: &str,
        account: &VirtualAccount,
        session_hash: Option<&str>,
    ) -> Result<bool> {
        let changed=sqlx::query("INSERT INTO oauth_browser_identities(flow_id,virtual_account_id,password_hash,session_hash) SELECT ?,id,password_hash,? FROM virtual_principals v WHERE id=? AND password_hash=? AND enabled=1 AND NOT EXISTS(SELECT 1 FROM user_subscriptions s JOIN user_identities u ON u.id=s.user_id WHERE s.virtual_account_id=v.id AND u.enabled=0) ON CONFLICT(flow_id) DO UPDATE SET virtual_account_id=excluded.virtual_account_id,password_hash=excluded.password_hash,session_hash=excluded.session_hash")
            .bind(flow).bind(session_hash).bind(&account.id).bind(&account.password_hash).execute(self.pool()).await?.rows_affected();
        Ok(changed == 1)
    }
    pub async fn browser_identity(&self, flow: &str) -> Result<Option<VirtualAccount>> {
        Ok(sqlx::query_as("SELECT v.* FROM oauth_browser_identities i JOIN oauth_browser_flows f ON f.id=i.flow_id JOIN virtual_principals v ON v.id=i.virtual_account_id WHERE i.flow_id=? AND f.expires_at>? AND v.enabled=1 AND v.password_hash=i.password_hash AND (i.session_hash IS NULL OR EXISTS(SELECT 1 FROM user_sessions s JOIN user_identities u ON u.id=s.user_id WHERE s.token_hash=i.session_hash AND s.expires_at>? AND u.enabled=1 AND s.user_id=(SELECT owner.user_id FROM user_subscriptions owner WHERE owner.virtual_account_id=v.id))) AND NOT EXISTS(SELECT 1 FROM user_subscriptions s JOIN user_identities u ON u.id=s.user_id WHERE s.virtual_account_id=v.id AND u.enabled=0)")
            .bind(flow).bind(Utc::now().timestamp()).bind(Utc::now().timestamp()).fetch_optional(self.pool()).await?)
    }
}

pub(crate) async fn confirmed_browser_identity(
    connection: &mut sqlx::SqliteConnection,
    flow: &str,
    account: &VirtualAccount,
) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM oauth_browser_identities i JOIN virtual_principals v ON v.id=i.virtual_account_id WHERE i.flow_id=? AND v.id=? AND v.password_hash=? AND i.password_hash=v.password_hash AND v.enabled=1 AND (i.session_hash IS NULL OR EXISTS(SELECT 1 FROM user_sessions s JOIN user_identities u ON u.id=s.user_id WHERE s.token_hash=i.session_hash AND s.expires_at>? AND u.enabled=1 AND s.user_id=(SELECT owner.user_id FROM user_subscriptions owner WHERE owner.virtual_account_id=v.id))) AND NOT EXISTS(SELECT 1 FROM user_subscriptions s JOIN user_identities u ON u.id=s.user_id WHERE s.virtual_account_id=v.id AND u.enabled=0))")
        .bind(flow).bind(&account.id).bind(&account.password_hash).bind(Utc::now().timestamp()).fetch_one(connection).await?)
}

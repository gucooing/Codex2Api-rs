//! User identity and browser sessions. These records never contain supplier data.
use crate::{PlatformAccount, Result, Storage, StorageError, hash_token};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum UserKind {
    Regular,
}

#[derive(Clone, FromRow)]
pub struct User {
    pub kind: UserKind,
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub name: String,
    pub email: String,
    pub enabled: bool,
    pub wallet_cents: i64,
    /// Identity revision; wallet movements do not change it.
    pub revision: i64,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct UserView {
    pub kind: UserKind,
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
            kind: self.kind,
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
    pub id: String,
    pub csrf_token: String,
    pub expires_at: i64,
    pub refresh_version: i64,
    pub refresh_issued_at: i64,
}

impl Storage {
    pub async fn allow_user_login_attempt(&self, username: &str) -> Result<bool> {
        static LOGIN: std::sync::LazyLock<redis::Script> = std::sync::LazyLock::new(|| {
            redis::Script::new(
                "local n=tonumber(redis.call('GET',KEYS[1]) or '0'); if n>=10 then return 0 end; n=redis.call('INCR',KEYS[1]); if n==1 then redis.call('PEXPIRE',KEYS[1],60000) end; return 1",
            )
        });
        let key = self.runtime.redis.key(&format!(
            "login-attempts:{}",
            hash_token(&username.to_lowercase())
        ));
        let allowed: i64 = self.runtime.redis.script(&LOGIN.key(key)).await?;
        Ok(allowed == 1)
    }
    pub async fn admin_overview(&self, tz_offset: i32) -> Result<serde_json::Value> {
        if !(-840..=840).contains(&tz_offset) {
            return Err(StorageError::InvalidAdminUpdate("时区无效"));
        }
        let now = Utc::now().timestamp_millis();
        let zone = i64::from(tz_offset) * 60_000;
        let start = (now - zone).div_euclid(86_400_000) * 86_400_000 + zone;
        let (suppliers,consumers,normal,users,active,models):(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT COUNT(*) FROM supplier_accounts),(SELECT COUNT(*) FROM regular_platforms),(SELECT COUNT(*) FROM regular_platforms v WHERE v.enabled=TRUE AND (v.subscription_expires_at IS NULL OR floor(extract(epoch FROM (v.subscription_expires_at)::timestamptz))::bigint>$1)),(SELECT COUNT(*) FROM regular_users),(SELECT COUNT(DISTINCT r.user_id) FROM usage_records r JOIN regular_users u ON u.id=r.user_id WHERE r.requested_at_ms>=$2 AND r.requested_at_ms<$3),(SELECT COUNT(*) FROM model_catalog WHERE deleted=FALSE)")
            .bind(now/1000).bind(start).bind(now+1).fetch_one(self.pool()).await?;
        Ok(
            serde_json::json!({"supplier_count":suppliers,"consumer_count":consumers,"normal_consumer_count":normal,"user_count":users,"active_user_count":active,"models_count":models,"activity_from_ms":start,"activity_until_ms":now+1}),
        )
    }
    pub async fn user_options(&self, search: &str) -> Result<serde_json::Value> {
        if search.len() > 128 {
            return Err(StorageError::InvalidAdminUpdate("用户搜索最多 128 字节"));
        }
        let rows:Vec<(String,String,String)> = sqlx::query_as("SELECT id,username,name FROM regular_users WHERE (strpos(lower(username),lower($1))>0 OR strpos(lower(name),lower($2))>0 OR strpos(lower(email),lower($3))>0) ORDER BY username,id LIMIT 5")
            .bind(search.trim()).bind(search.trim()).bind(search.trim()).fetch_all(self.pool()).await?;
        Ok(
            serde_json::json!({"items":rows.into_iter().map(|(id,username,name)|serde_json::json!({"id":id,"username":username,"name":name})).collect::<Vec<_>>()}),
        )
    }
    pub async fn clear_browser_identity(&self, flow: &str) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM oauth_browser_identities WHERE flow_id=$1")
            .bind(flow)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM oauth_browser_users WHERE flow_id=$1")
            .bind(flow)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn users(&self) -> Result<Vec<User>> {
        Ok(
            sqlx::query_as("SELECT * FROM regular_users ORDER BY created_at DESC,id")
                .fetch_all(self.pool())
                .await?,
        )
    }
    pub async fn user(&self, id: &str) -> Result<Option<User>> {
        Ok(sqlx::query_as("SELECT * FROM regular_users WHERE id=$1")
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }
    /// Shared platform administration reads its actual owner; ordinary user reads stay scoped.
    pub async fn platform_account_user(&self, platform_id: &str) -> Result<Option<User>> {
        Ok(sqlx::query_as("SELECT u.* FROM user_identities u JOIN platform_accounts p ON p.user_id=u.id WHERE p.id=$1")
            .bind(platform_id).fetch_optional(self.pool()).await?)
    }
    pub async fn user_by_username(&self, username: &str) -> Result<Option<User>> {
        Ok(sqlx::query_as(
            "SELECT * FROM user_identities WHERE lower(username)=lower($1)  AND enabled=TRUE",
        )
        .bind(username)
        .fetch_optional(self.pool())
        .await?)
    }
    pub async fn save_user(&self, user: &User, expected: Option<i64>) -> Result<bool> {
        let mut tx = self.pool().begin().await?;
        let changed = write_identity(&mut tx, user, expected).await?;
        tx.commit().await?;
        if changed {
            self.runtime
                .snapshots
                .invalidate(&[crate::snapshots::Domain::Subscriptions]);
        }
        Ok(changed)
    }
    pub async fn create_user_session(&self, user: &User, csrf: &str) -> Result<Option<String>> {
        let now = Utc::now().timestamp();
        let expires = now + i64::from(crate::WEB_REFRESH_TTL_SECONDS);
        let id = uuid::Uuid::new_v4().to_string();
        let token = self
            .sign_jwt(
                crate::TokenPurpose::UserSession,
                serde_json::json!({
                    "sub":user.id,"jti":id,"iat":now,"exp":now+i64::from(crate::WEB_ACCESS_TTL_SECONDS)
                }),
            )
            .await?;
        let mut tx = self.pool().begin().await?;
        sqlx::query("SELECT id FROM accounts WHERE id=$1 FOR SHARE")
            .bind(&user.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM user_sessions WHERE expires_at<=$1")
            .bind(Utc::now().timestamp())
            .execute(&mut *tx)
            .await?;
        let inserted=sqlx::query("INSERT INTO user_sessions(id,user_id,csrf_token,expires_at,refresh_issued_at) SELECT $1,id,$2,$3,floor(extract(epoch FROM clock_timestamp()))::bigint FROM regular_users WHERE id=$4 AND enabled=TRUE AND password_hash=$5")
            .bind(id).bind(csrf).bind(expires)
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
        Ok(sqlx::query_as("SELECT s.* FROM user_sessions s JOIN regular_users u ON u.id=s.user_id WHERE s.id=$1 AND s.expires_at>$2 AND u.enabled=TRUE AND u.id=$3")
            .bind(claims["jti"].as_str()).bind(Utc::now().timestamp()).bind(claims["sub"].as_str()).fetch_optional(self.pool()).await?)
    }
    pub async fn revoke_user_session(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM user_sessions WHERE id=$1")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn user_session_tokens(
        &self,
        session: &UserSession,
    ) -> Result<crate::WebSessionTokens> {
        self.web_session_tokens(
            crate::TokenPurpose::UserSession,
            crate::TokenPurpose::UserRefresh,
            &session.user_id,
            &session.id,
            session.expires_at,
            (session.refresh_version, session.refresh_issued_at),
        )
        .await
    }

    pub async fn user_refresh_session(&self, token: &str) -> Result<Option<UserSession>> {
        let claims = match self
            .verify_jwt(crate::TokenPurpose::UserRefresh, token)
            .await
        {
            Ok(claims) => claims,
            Err(StorageError::InvalidJwt) => return Ok(None),
            Err(error) => return Err(error),
        };
        let Some(version) = claims["version"].as_i64().filter(|v| *v >= 0) else {
            return Ok(None);
        };
        Ok(sqlx::query_as(
            "SELECT s.* FROM user_sessions s JOIN regular_users u ON u.id=s.user_id
             WHERE s.id=$1 AND s.user_id=$2 AND s.expires_at>$3 AND u.enabled=TRUE
             AND (s.refresh_version=$4 OR (s.refresh_version-1=$5 AND s.refresh_issued_at>$6))",
        )
        .bind(claims["jti"].as_str())
        .bind(claims["sub"].as_str())
        .bind(Utc::now().timestamp())
        .bind(version)
        .bind(version)
        .bind(Utc::now().timestamp() - crate::web_sessions::REFRESH_GRACE_SECONDS)
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn renew_user_session(&self, session: &UserSession) -> Result<Option<UserSession>> {
        let now = Utc::now().timestamp();
        let cutoff = now - crate::web_sessions::REFRESH_GRACE_SECONDS;
        Ok(sqlx::query_as(
            "UPDATE user_sessions SET
             expires_at=CASE WHEN refresh_issued_at<=$1 THEN $2 ELSE expires_at END,
             refresh_version=refresh_version+CASE WHEN refresh_issued_at<=$3 THEN 1 ELSE 0 END,
             refresh_issued_at=CASE WHEN refresh_issued_at<=$4 THEN $5 ELSE refresh_issued_at END
             WHERE id=$6 AND user_id=$7 AND expires_at>$8
             AND (refresh_version=$9 OR (refresh_version-1=$10 AND refresh_issued_at>$11))
             AND EXISTS(SELECT 1 FROM regular_users WHERE id=user_id AND enabled=TRUE)
             RETURNING *",
        )
        .bind(cutoff)
        .bind(now + i64::from(crate::WEB_REFRESH_TTL_SECONDS))
        .bind(cutoff)
        .bind(cutoff)
        .bind(now)
        .bind(&session.id)
        .bind(&session.user_id)
        .bind(now)
        .bind(session.refresh_version)
        .bind(session.refresh_version)
        .bind(cutoff)
        .fetch_optional(self.pool())
        .await?)
    }
    pub async fn oauth_subscriptions(
        &self,
        user: &str,
        provider: &str,
    ) -> Result<Vec<PlatformAccount>> {
        Ok(sqlx::query_as("SELECT * FROM regular_platforms WHERE user_id=$1 AND provider_id=$2 AND enabled=TRUE AND (subscription_expires_at IS NULL OR floor(extract(epoch FROM (subscription_expires_at)::timestamptz))::bigint>$3) ORDER BY created_at,id")
            .bind(user).bind(provider).bind(Utc::now().timestamp()).fetch_all(self.pool()).await?)
    }
    pub async fn bind_browser_user(
        &self,
        flow: &str,
        user: &User,
        session: Option<&str>,
    ) -> Result<bool> {
        Ok(sqlx::query("INSERT INTO oauth_browser_users(flow_id,user_id,password_hash,session_id) SELECT $1,id,password_hash,$2 FROM regular_users WHERE id=$3 AND enabled=TRUE AND password_hash=$4 ON CONFLICT(flow_id) DO UPDATE SET user_id=excluded.user_id,password_hash=excluded.password_hash,session_id=excluded.session_id")
            .bind(flow).bind(session).bind(&user.id).bind(&user.password_hash).execute(self.pool()).await?.rows_affected()==1)
    }
    pub async fn browser_user(&self, flow: &str) -> Result<Option<User>> {
        Ok(sqlx::query_as("SELECT u.* FROM oauth_browser_users b JOIN oauth_browser_flows f ON f.id=b.flow_id JOIN regular_users u ON u.id=b.user_id WHERE b.flow_id=$1 AND f.expires_at>$2 AND u.enabled=TRUE AND u.password_hash=b.password_hash AND (b.session_id IS NULL OR EXISTS(SELECT 1 FROM user_sessions s WHERE s.id=b.session_id AND s.user_id=u.id AND s.expires_at>$3))")
            .bind(flow).bind(Utc::now().timestamp()).bind(Utc::now().timestamp()).fetch_optional(self.pool()).await?)
    }
    pub async fn select_browser_subscription(
        &self,
        flow: &str,
        id: &str,
        provider: &str,
    ) -> Result<Option<PlatformAccount>> {
        let Some(user) = self.browser_user(flow).await? else {
            return Ok(None);
        };
        let account: Option<PlatformAccount> = sqlx::query_as("SELECT * FROM regular_platforms WHERE id=$1 AND user_id=$2 AND provider_id=$3 AND enabled=TRUE AND (subscription_expires_at IS NULL OR floor(extract(epoch FROM (subscription_expires_at)::timestamptz))::bigint>$4)")
            .bind(id).bind(&user.id).bind(provider).bind(Utc::now().timestamp()).fetch_optional(self.pool()).await?;
        let Some(account) = account else {
            return Ok(None);
        };
        let session: Option<String> =
            sqlx::query_scalar("SELECT session_id FROM oauth_browser_users WHERE flow_id=$1")
                .bind(flow)
                .fetch_one(self.pool())
                .await?;
        if !self
            .bind_browser_identity(flow, &account, session.as_deref())
            .await?
        {
            return Ok(None);
        }
        Ok(Some(account))
    }

    pub async fn platform_account_owner(&self, account: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT user_id FROM platform_accounts WHERE id=$1")
                .bind(account)
                .fetch_optional(self.pool())
                .await?,
        )
    }
    pub async fn platform_owner_enabled(&self, account: &str) -> Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM platform_principals WHERE id=$1 AND enabled=TRUE)",
        )
        .bind(account)
        .fetch_one(self.pool())
        .await?)
    }
    pub async fn bind_browser_identity(
        &self,
        flow: &str,
        account: &PlatformAccount,
        session_id: Option<&str>,
    ) -> Result<bool> {
        let changed=sqlx::query("INSERT INTO oauth_browser_identities(flow_id,virtual_account_id,password_hash,session_id) SELECT $1,id,password_hash,$2 FROM platform_principals v WHERE id=$3 AND password_hash=$4 AND enabled=TRUE AND EXISTS(SELECT 1 FROM oauth_browser_users b WHERE b.flow_id=$5 AND b.user_id=v.user_id AND b.password_hash=v.password_hash AND b.session_id IS NOT DISTINCT FROM $6) ON CONFLICT(flow_id) DO UPDATE SET virtual_account_id=excluded.virtual_account_id,password_hash=excluded.password_hash,session_id=excluded.session_id")
            .bind(flow).bind(session_id).bind(&account.id).bind(&account.password_hash).bind(flow).bind(session_id).execute(self.pool()).await?.rows_affected();
        Ok(changed == 1)
    }
    pub async fn browser_identity(&self, flow: &str) -> Result<Option<PlatformAccount>> {
        Ok(sqlx::query_as("SELECT v.* FROM oauth_browser_identities i JOIN oauth_browser_flows f ON f.id=i.flow_id JOIN platform_principals v ON v.id=i.virtual_account_id WHERE i.flow_id=$1 AND f.expires_at>$2 AND v.enabled=TRUE AND v.password_hash=i.password_hash AND (i.session_id IS NULL OR EXISTS(SELECT 1 FROM user_sessions s JOIN regular_users u ON u.id=s.user_id WHERE s.id=i.session_id AND s.expires_at>$3 AND u.enabled=TRUE AND s.user_id=v.user_id))")
            .bind(flow).bind(Utc::now().timestamp()).bind(Utc::now().timestamp()).fetch_optional(self.pool()).await?)
    }
}

pub(crate) async fn confirmed_browser_identity(
    connection: &mut sqlx::PgConnection,
    flow: &str,
    account: &PlatformAccount,
) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM oauth_browser_identities i JOIN platform_principals v ON v.id=i.virtual_account_id WHERE i.flow_id=$1 AND v.id=$2 AND v.password_hash=$3 AND i.password_hash=v.password_hash AND v.enabled=TRUE AND (v.subscription_expires_at IS NULL OR floor(extract(epoch FROM (v.subscription_expires_at)::timestamptz))::bigint>floor(extract(epoch FROM clock_timestamp()))::bigint) AND EXISTS(SELECT 1 FROM oauth_browser_users b WHERE b.flow_id=i.flow_id AND b.user_id=v.user_id AND b.password_hash=v.password_hash) AND (i.session_id IS NULL OR EXISTS(SELECT 1 FROM user_sessions s JOIN regular_users u ON u.id=s.user_id WHERE s.id=i.session_id AND s.expires_at>$4 AND u.enabled=TRUE AND s.user_id=v.user_id)))")
        .bind(flow).bind(&account.id).bind(&account.password_hash).bind(Utc::now().timestamp()).fetch_one(connection).await?)
}

pub(crate) async fn write_identity(
    connection: &mut sqlx::PgConnection,
    user: &User,
    expected: Option<i64>,
) -> Result<bool> {
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
    let now = Utc::now().to_rfc3339();
    let changed = if let Some(revision) = expected {
        crate::account_scope::require_on(connection, &user.id, crate::AccountScope::User).await?;
        let changed=sqlx::query("UPDATE users SET name=$1,email=$2,revision=revision+1 WHERE id=$3 AND revision=$4 AND kind=$5 AND EXISTS(SELECT 1 FROM accounts WHERE accounts.id=users.id AND account_type='user')")
                .bind(user.name.trim()).bind(user.email.trim()).bind(&user.id).bind(revision).bind(user.kind).execute(&mut *connection).await?.rows_affected();
        if changed == 1 {
            let account=sqlx::query("UPDATE accounts SET username=$1,password_hash=$2,enabled=$3,updated_at=$4 WHERE id=$5 AND account_type='user'")
                    .bind(user.username.trim()).bind(&user.password_hash).bind(user.enabled).bind(&now).bind(&user.id).execute(&mut *connection).await?.rows_affected();
            if account != 1 {
                return Err(StorageError::InvalidCredentials);
            }
        }
        changed
    } else {
        sqlx::query("INSERT INTO accounts(id,account_type,username,password_hash,enabled,created_at,updated_at) VALUES($1,'user',$2,$3,$4,$5,$6)")
                .bind(&user.id).bind(user.username.trim()).bind(&user.password_hash).bind(user.enabled).bind(&user.created_at).bind(&now).execute(&mut *connection).await?;
        sqlx::query("INSERT INTO users(id,name,email,kind) VALUES($1,$2,$3,$4)")
            .bind(&user.id)
            .bind(user.name.trim())
            .bind(user.email.trim())
            .bind(user.kind)
            .execute(&mut *connection)
            .await?
            .rows_affected()
    };
    Ok(changed == 1)
}

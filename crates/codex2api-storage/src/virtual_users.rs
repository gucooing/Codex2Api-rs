//! Administrator-managed OAuth-only users. Identity writes use the same user store.
use crate::{PlatformAccount, Result, Storage, StorageError, User, UserKind, VirtualPlan};

pub struct VirtualUserChange<'a> {
    pub user: &'a User,
    pub platform_id: &'a str,
    pub provider_id: &'a str,
    pub plan_id: &'a str,
    pub expires_at: Option<&'a str>,
    pub user_revision: Option<i64>,
    pub revision: Option<i64>,
}

impl Storage {
    pub async fn virtual_user(&self, platform_id: &str) -> Result<Option<User>> {
        Ok(sqlx::query_as("SELECT u.* FROM virtual_users u JOIN platform_accounts p ON p.user_id=u.id WHERE p.id=?")
            .bind(platform_id).fetch_optional(self.pool()).await?)
    }

    pub async fn save_virtual_user(&self, input: VirtualUserChange<'_>) -> Result<()> {
        if input.user.kind != UserKind::Virtual {
            return Err(StorageError::InvalidAdminUpdate(
                "虚拟账户须使用虚拟用户身份",
            ));
        }
        let expires = input
            .expires_at
            .map(|value| chrono::DateTime::parse_from_rfc3339(value).map(|v| v.to_rfc3339()))
            .transpose()
            .map_err(|_| StorageError::InvalidAdminUpdate("订阅到期时间无效"))?;
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let current: Option<(String, String, i64)> =
            sqlx::query_as("SELECT user_id,provider_id,revision FROM platform_accounts WHERE id=?")
                .bind(input.platform_id)
                .fetch_optional(&mut *tx)
                .await?;
        if current.as_ref().map(|v| v.2) != input.revision
            || current
                .as_ref()
                .is_some_and(|v| v.0 != input.user.id || v.1 != input.provider_id)
            || current.is_some() != input.user_revision.is_some()
        {
            return Err(StorageError::ProxyChanged);
        }
        let previous: Option<PlatformAccount> =
            sqlx::query_as("SELECT * FROM platform_principals WHERE id=?")
                .bind(input.platform_id)
                .fetch_optional(&mut *tx)
                .await?;
        let plan: VirtualPlan =
            sqlx::query_as("SELECT * FROM virtual_plans WHERE id=? AND provider_id=?")
                .bind(input.plan_id)
                .bind(input.provider_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StorageError::InvalidAdminUpdate("请选择可用的同提供商套餐"))?;
        if !crate::users::write_identity(&mut tx, input.user, input.user_revision).await? {
            return Err(StorageError::ProxyChanged);
        }
        crate::user_subscriptions::write_subscription(
            &mut tx,
            input.user,
            &plan,
            previous.as_ref(),
            expires.as_deref(),
            input.user.enabled,
            "admin",
            Some(input.platform_id),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn delete_virtual_user(&self, platform_id: &str) -> Result<()> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        delete_on(&mut tx, platform_id).await?;
        tx.commit().await?;
        Ok(())
    }
}

pub(crate) async fn delete_on(
    connection: &mut sqlx::SqliteConnection,
    platform_id: &str,
) -> Result<()> {
    let owner: String = sqlx::query_scalar("SELECT p.user_id FROM platform_accounts p JOIN users u ON u.id=p.user_id WHERE p.id=? AND u.kind='virtual'")
        .bind(platform_id).fetch_optional(&mut *connection).await?
        .ok_or(StorageError::InvalidAdminUpdate("只能在虚拟账户管理中删除虚拟用户"))?;
    sqlx::query("DELETE FROM platform_accounts WHERE id=?")
        .bind(platform_id)
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM accounts WHERE id=? AND account_type='user'")
        .bind(owner)
        .execute(connection)
        .await?;
    Ok(())
}

//! One persisted entitlement per user/provider. Wallet debits and grants commit together.
use crate::{Result, Storage, StorageError, User, VirtualAccount, VirtualPlan};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{FromRow, SqliteConnection};

#[derive(Clone, FromRow, Serialize)]
pub struct UserSubscription {
    pub user_id: String,
    pub username: String,
    pub name: String,
    pub virtual_account_id: String,
    pub provider_id: String,
    pub plan_id: String,
    pub plan_name: String,
    pub plan_type: String,
    pub subscription_expires_at: Option<String>,
    pub enabled: bool,
    pub revision: i64,
    pub created_at: String,
}
impl UserSubscription {
    pub fn expired(&self) -> bool {
        self.subscription_expires_at.as_deref().is_some_and(|v| {
            DateTime::parse_from_rfc3339(v)
                .ok()
                .is_none_or(|t| t <= Utc::now())
        })
    }
    /// Explicit user contract; never serialize an administrator DTO or execution route here.
    pub fn user_view(&self) -> serde_json::Value {
        json!({"id":self.virtual_account_id,"provider_id":self.provider_id,"plan_id":self.plan_id,
            "plan_name":self.plan_name,"plan_type":self.plan_type,"expires_at":self.subscription_expires_at,
            "enabled":self.enabled,"expired":self.expired(),"revision":self.revision})
    }
}
#[derive(Clone, FromRow, Serialize)]
pub struct WalletEntry {
    pub id: String,
    pub request_id: String,
    pub kind: String,
    pub plan_id: Option<String>,
    pub plan_name: Option<String>,
    pub provider_id: Option<String>,
    pub amount_cents: i64,
    pub balance_cents: i64,
    pub balance_before_cents: i64,
    pub duration_days: Option<i64>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub operator_account_id: Option<String>,
    pub operator_name: Option<String>,
    pub reason: Option<String>,
    pub order_id: Option<String>,
}
impl WalletEntry {
    pub fn user_view(&self) -> serde_json::Value {
        json!({"id":self.id,"kind":self.kind,"plan_name":self.plan_name,"amount_cents":self.amount_cents,
            "balance_before_cents":self.balance_before_cents,"balance_cents":self.balance_cents,"duration_days":self.duration_days,
            "created_at":self.created_at,"expires_at":self.expires_at})
    }
}
pub struct SubscriptionChange<'a> {
    pub reissue: bool,
    pub user_id: &'a str,
    pub plan_id: &'a str,
    pub expires_at: Option<&'a str>,
    pub enabled: bool,
    pub revision: Option<i64>,
}
impl VirtualPlan {
    pub fn sale_price_cents(&self) -> Result<Option<i64>> {
        match self.config.get("sale_price_usd").filter(|v| !v.is_null()) {
            None => Ok(None),
            Some(value) => value
                .as_str()
                .and_then(|v| crate::decimal_units(v, 2))
                .map(Some)
                .ok_or(StorageError::InvalidAdminUpdate(
                    "售价须为非负美元金额，最多两位小数",
                )),
        }
    }
    pub fn duration_days(&self) -> Result<i64> {
        self.config
            .get("duration_days")
            .map_or(Some(30), |v| v.as_i64())
            .filter(|v| (1..=3650).contains(v))
            .ok_or(StorageError::InvalidAdminUpdate(
                "套餐有效时长须为 1 至 3650 天",
            ))
    }
}
const SUBSCRIPTIONS: &str = "SELECT s.user_id,u.username,u.name,s.virtual_account_id,s.provider_id,v.plan_id,p.name AS plan_name,v.plan_type,v.subscription_expires_at,v.enabled,s.revision,v.created_at FROM user_subscriptions s JOIN user_identities u ON u.id=s.user_id JOIN virtual_accounts v ON v.id=s.virtual_account_id JOIN virtual_plans p ON p.id=v.plan_id";

impl Storage {
    pub async fn user_subscriptions(
        &self,
        user: Option<&str>,
        include_expired: bool,
    ) -> Result<Vec<UserSubscription>> {
        self.filter_user_subscriptions(user, None, include_expired)
            .await
    }
    pub async fn filter_user_subscriptions(
        &self,
        user: Option<&str>,
        plan: Option<&str>,
        include_expired: bool,
    ) -> Result<Vec<UserSubscription>> {
        if user.is_some_and(|id| id.len() > 128) || plan.is_some_and(|id| id.len() > 128) {
            return Err(StorageError::InvalidAdminUpdate("订阅筛选条件无效"));
        }
        Ok(sqlx::query_as(&format!("{SUBSCRIPTIONS} WHERE (? IS NULL OR s.user_id=?) AND (? IS NULL OR v.plan_id=?) AND (? OR v.subscription_expires_at IS NULL OR unixepoch(v.subscription_expires_at)>?) ORDER BY v.created_at DESC,s.user_id,s.provider_id"))
            .bind(user).bind(user).bind(plan).bind(plan).bind(include_expired).bind(Utc::now().timestamp()).fetch_all(self.pool()).await?)
    }
    pub async fn wallet_entries(&self, user: &str) -> Result<Vec<WalletEntry>> {
        Ok(sqlx::query_as(
            "SELECT * FROM wallet_entries WHERE user_id=? ORDER BY created_at DESC,id",
        )
        .bind(user)
        .fetch_all(self.pool())
        .await?)
    }
    pub async fn save_user_subscription(&self, input: SubscriptionChange<'_>) -> Result<()> {
        let expires = input
            .expires_at
            .map(|v| DateTime::parse_from_rfc3339(v).map(|v| v.to_rfc3339()))
            .transpose()
            .map_err(|_| StorageError::InvalidAdminUpdate("订阅到期时间无效"))?;
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let user: User = sqlx::query_as("SELECT * FROM user_identities WHERE id=?")
            .bind(input.user_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(input.user_id.into()))?;
        let plan: VirtualPlan = sqlx::query_as("SELECT * FROM virtual_plans WHERE id=?")
            .bind(input.plan_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StorageError::InvalidAdminUpdate("请选择可用套餐"))?;
        let current = subscription_account(&mut tx, &user.id, &plan.provider_id).await?;
        if current.as_ref().map(|(_, r)| *r) != input.revision {
            return Err(StorageError::ProxyChanged);
        }
        let account_id = write_subscription(
            &mut tx,
            &user,
            &plan,
            current.as_ref().map(|(a, _)| a),
            if plan.plan_type == "free" {
                None
            } else {
                expires.as_deref()
            },
            input.enabled,
            if input.reissue {
                "admin_reissue"
            } else {
                "admin"
            },
        )
        .await?;
        crate::subscription_orders::record_admin_pricing(
            &mut tx,
            &account_id,
            &plan,
            if input.reissue {
                None
            } else {
                current.as_ref().map(|(account, _)| account)
            },
            if plan.plan_type == "free" {
                None
            } else {
                expires.as_deref()
            },
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
pub(crate) async fn subscription_account(
    connection: &mut SqliteConnection,
    user: &str,
    provider: &str,
) -> Result<Option<(VirtualAccount, i64)>> {
    let row:Option<(String,i64)>=sqlx::query_as("SELECT virtual_account_id,revision FROM user_subscriptions WHERE user_id=? AND provider_id=?")
        .bind(user).bind(provider).fetch_optional(&mut *connection).await?;
    let Some((id, revision)) = row else {
        return Ok(None);
    };
    Ok(Some((
        sqlx::query_as("SELECT * FROM virtual_accounts WHERE id=?")
            .bind(id)
            .fetch_one(connection)
            .await?,
        revision,
    )))
}
pub(crate) async fn write_subscription(
    connection: &mut SqliteConnection,
    user: &User,
    plan: &VirtualPlan,
    previous: Option<&VirtualAccount>,
    expires: Option<&str>,
    enabled: bool,
    origin: &str,
) -> Result<String> {
    let id = previous
        .map(|a| a.id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = Utc::now();
    let plan_changed = previous.is_some_and(|a| a.plan_id != plan.id);
    let restart = origin == "admin_reissue"
        || (origin != "order_upgrade"
            && previous.is_none_or(|a| {
                a.plan_id != plan.id
                    || a.subscription_expires_at.as_deref().is_some_and(|v| {
                        DateTime::parse_from_rfc3339(v)
                            .ok()
                            .is_none_or(|e| e <= now)
                    })
            }));
    if previous.is_none() {
        sqlx::query("INSERT INTO virtual_accounts(id,provider_id,username,password_hash,name,email,plan_id,plan_type,subscription_expires_at,subscription_started_at,enabled,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(&plan.provider_id).bind(format!("user-{}",uuid::Uuid::new_v4().simple()))
            .bind("").bind("").bind("").bind(&plan.id).bind(&plan.plan_type)
            .bind(expires).bind(now.to_rfc3339()).bind(enabled).bind(now.to_rfc3339()).execute(&mut *connection).await?;
        sqlx::query(
            "INSERT INTO user_subscriptions(user_id,provider_id,virtual_account_id) VALUES(?,?,?)",
        )
        .bind(&user.id)
        .bind(&plan.provider_id)
        .bind(&id)
        .execute(&mut *connection)
        .await?;
    } else {
        sqlx::query("UPDATE virtual_accounts SET plan_id=?,plan_type=?,subscription_expires_at=?,enabled=?,subscription_started_at=CASE WHEN ? THEN ? ELSE subscription_started_at END WHERE id=?")
            .bind(&plan.id).bind(&plan.plan_type).bind(expires).bind(enabled).bind(restart).bind(now.to_rfc3339()).bind(&id).execute(&mut *connection).await?;
    }
    if !enabled {
        sqlx::query("DELETE FROM virtual_devices WHERE virtual_account_id=?")
            .bind(&id)
            .execute(&mut *connection)
            .await?;
        sqlx::query("DELETE FROM virtual_authorization_codes WHERE virtual_account_id=?")
            .bind(&id)
            .execute(&mut *connection)
            .await?;
        sqlx::query("DELETE FROM oauth_browser_identities WHERE virtual_account_id=?")
            .bind(&id)
            .execute(&mut *connection)
            .await?;
    }
    if restart || plan_changed {
        let tag = plan.config["supplier_tag_id"].as_str();
        sqlx::query("INSERT INTO execution_routes(virtual_account_id,provider_id,tag_id,supplier_account_id) VALUES(?,?,?,NULL) ON CONFLICT(virtual_account_id,provider_id) DO UPDATE SET tag_id=excluded.tag_id,supplier_account_id=NULL,revision=revision+1")
            .bind(&id).bind(&plan.provider_id).bind(tag).execute(&mut *connection).await?;
    }
    let operation = if origin == "admin_reissue" {
        "reissue"
    } else if previous.is_none() {
        "grant"
    } else if previous.is_some_and(|a| a.plan_id != plan.id) {
        "change_plan"
    } else if previous.is_some_and(|a| a.enabled != enabled) {
        if enabled { "enable" } else { "disable" }
    } else if previous.is_some_and(|a| {
        expires.is_none() && a.subscription_expires_at.is_some()
            || a.subscription_expires_at
                .as_deref()
                .and_then(|old| DateTime::parse_from_rfc3339(old).ok())
                .zip(expires.and_then(|new| DateTime::parse_from_rfc3339(new).ok()))
                .is_some_and(|(old, new)| new > old)
    }) {
        "renew"
    } else {
        "change_expiry"
    };
    let previous_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM virtual_plans WHERE id=?")
            .bind(previous.map(|account| &account.plan_id))
            .fetch_optional(&mut *connection)
            .await?;
    let value = json!({"operation":operation,"origin":origin,"plan_type":plan.plan_type,"plan_id":plan.id,"plan_name":plan.name,
        "previous_plan_id":previous.map(|a|&a.plan_id),"previous_plan_name":previous_name,"previous_expires_at":previous.and_then(|a|a.subscription_expires_at.as_ref()),"expires_at":expires,"created_at_ms":now.timestamp_millis()});
    sqlx::query("INSERT INTO virtual_resources(virtual_account_id,kind,id,value_json,created_at_ms,updated_at_ms) VALUES(?,'subscription_operation',?,?,?,?)")
        .bind(&id).bind(uuid::Uuid::new_v4().to_string()).bind(value.to_string()).bind(now.timestamp_millis()).bind(now.timestamp_millis()).execute(connection).await?;
    Ok(id)
}

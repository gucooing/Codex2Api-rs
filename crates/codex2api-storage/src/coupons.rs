use crate::{Result, Storage, StorageError};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqliteConnection};

pub(crate) fn checkout_error(code: &'static str, message: &'static str) -> StorageError {
    StorageError::Checkout { code, message }
}

#[derive(Clone, Debug, Deserialize, Serialize, FromRow)]
pub struct Coupon {
    pub id: String,
    pub code: String,
    pub name: String,
    pub enabled: bool,
    pub discount_cents: i64,
    pub minimum_cents: i64,
    pub plan_id: Option<String>,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub max_uses: Option<i64>,
    pub per_user_limit: i64,
    pub revision: i64,
    pub created_at_ms: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CouponInput {
    pub code: String,
    pub name: String,
    pub enabled: bool,
    pub discount_cents: i64,
    pub minimum_cents: i64,
    pub plan_id: Option<String>,
    pub starts_at_ms: i64,
    pub ends_at_ms: i64,
    pub max_uses: Option<i64>,
    pub per_user_limit: i64,
    pub revision: Option<i64>,
}

impl Storage {
    pub async fn coupons(&self) -> Result<serde_json::Value> {
        let now = Utc::now().timestamp_millis();
        let rows: Vec<Coupon> =
            sqlx::query_as("SELECT * FROM coupons ORDER BY created_at_ms DESC,id")
                .fetch_all(self.pool())
                .await?;
        let mut items = Vec::new();
        for coupon in rows {
            let (paid, reserved): (i64,i64) = sqlx::query_as("SELECT COALESCE(SUM(status='paid'),0),COALESCE(SUM(status='pending' AND quote_expires_at_ms>?),0) FROM subscription_orders WHERE coupon_id=?")
                .bind(now).bind(&coupon.id).fetch_one(self.pool()).await?;
            let mut value = serde_json::to_value(coupon)?;
            value["used_count"] = paid.into();
            value["reserved_count"] = reserved.into();
            items.push(value);
        }
        Ok(serde_json::json!({"items":items}))
    }

    pub async fn save_coupon(&self, id: Option<&str>, input: CouponInput) -> Result<Coupon> {
        let code = input.code.trim().to_ascii_uppercase();
        if !(3..=64).contains(&code.len())
            || !code
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(checkout_error(
                "invalid_coupon_code",
                "优惠码须为 3–64 位字母、数字、短横线或下划线",
            ));
        }
        if input.name.trim().is_empty()
            || input.name.len() > 120
            || input.discount_cents <= 0
            || input.discount_cents > 100_000_000
            || input.minimum_cents < 0
            || input.minimum_cents > 100_000_000
            || input.starts_at_ms < 0
            || input.ends_at_ms <= input.starts_at_ms
            || input.ends_at_ms > 253_402_300_799_000
            || input.max_uses.is_some_and(|n| n <= 0)
            || input.per_user_limit <= 0
        {
            return Err(checkout_error(
                "invalid_coupon",
                "请检查优惠券名称、金额、有效期和使用次数",
            ));
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        if let Some(plan) = &input.plan_id {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM virtual_plans WHERE id=? AND plan_type!='free')",
            )
            .bind(plan)
            .fetch_one(&mut *tx)
            .await?;
            if !exists {
                return Err(checkout_error(
                    "invalid_coupon_plan",
                    "优惠券只能关联现有付费套餐",
                ));
            }
        }
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM coupons WHERE code=? AND (? IS NULL OR id!=?))",
        )
        .bind(&code)
        .bind(id)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if duplicate {
            return Err(checkout_error("duplicate_coupon", "优惠码已存在"));
        }
        let key = id
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if id.is_some() {
            let rows = sqlx::query("UPDATE coupons SET code=?,name=?,enabled=?,discount_cents=?,minimum_cents=?,plan_id=?,starts_at_ms=?,ends_at_ms=?,max_uses=?,per_user_limit=?,revision=revision+1 WHERE id=? AND revision=?")
                .bind(&code).bind(input.name.trim()).bind(input.enabled).bind(input.discount_cents).bind(input.minimum_cents).bind(&input.plan_id).bind(input.starts_at_ms).bind(input.ends_at_ms).bind(input.max_uses).bind(input.per_user_limit).bind(&key).bind(input.revision).execute(&mut *tx).await?.rows_affected();
            if rows != 1 {
                return Err(StorageError::ProxyChanged);
            }
        } else {
            if input.revision.is_some() {
                return Err(checkout_error("invalid_coupon", "新建优惠券不能指定版本"));
            }
            sqlx::query("INSERT INTO coupons(id,code,name,enabled,discount_cents,minimum_cents,plan_id,starts_at_ms,ends_at_ms,max_uses,per_user_limit,created_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(&key).bind(&code).bind(input.name.trim()).bind(input.enabled).bind(input.discount_cents).bind(input.minimum_cents).bind(&input.plan_id).bind(input.starts_at_ms).bind(input.ends_at_ms).bind(input.max_uses).bind(input.per_user_limit).bind(Utc::now().timestamp_millis()).execute(&mut *tx).await?;
        }
        let result = sqlx::query_as("SELECT * FROM coupons WHERE id=?")
            .bind(key)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result)
    }
}

// A preview only reads availability. Confirmed unpaid orders reserve capacity;
// cancellation and expiry release it without a cleanup job. Paid orders redeem it.
pub(crate) async fn available_coupon(
    connection: &mut SqliteConnection,
    owner: &str,
    code: &str,
    plan: &str,
    subtotal: i64,
    now: i64,
) -> Result<Option<Coupon>> {
    let code = code.trim();
    if code.is_empty() {
        return Ok(None);
    }
    if code.len() > 64 {
        return Err(checkout_error("coupon_invalid", "优惠码无效"));
    }
    let coupon: Coupon = sqlx::query_as("SELECT * FROM coupons WHERE code=?")
        .bind(code)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| checkout_error("coupon_invalid", "优惠码不存在"))?;
    if !coupon.enabled {
        return Err(checkout_error("coupon_disabled", "该优惠券已停用"));
    }
    if now < coupon.starts_at_ms {
        return Err(checkout_error("coupon_not_started", "该优惠券尚未生效"));
    }
    if now >= coupon.ends_at_ms {
        return Err(checkout_error("coupon_expired", "该优惠券已过期"));
    }
    if coupon.plan_id.as_deref().is_some_and(|id| id != plan) {
        return Err(checkout_error(
            "coupon_plan_mismatch",
            "该优惠券不适用于所选套餐",
        ));
    }
    if subtotal < coupon.minimum_cents {
        return Err(checkout_error(
            "coupon_minimum",
            "抵扣原订阅后的金额未达到优惠券使用门槛",
        ));
    }
    if subtotal == 0 {
        return Err(checkout_error(
            "coupon_not_needed",
            "当前应付金额为零，无需使用优惠券",
        ));
    }
    let (total, own): (i64,i64) = sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(user_id=?),0) FROM subscription_orders WHERE coupon_id=? AND (status='paid' OR (status='pending' AND quote_expires_at_ms>?))")
        .bind(owner).bind(&coupon.id).bind(now).fetch_one(connection).await?;
    if coupon.max_uses.is_some_and(|max| total >= max) {
        return Err(checkout_error(
            "coupon_exhausted",
            "该优惠券可用次数已用完或被待支付订单占用",
        ));
    }
    if own >= coupon.per_user_limit {
        return Err(checkout_error(
            "coupon_user_limit",
            "已达到该优惠券的个人使用次数；待支付订单也会占用次数",
        ));
    }
    Ok(Some(coupon))
}

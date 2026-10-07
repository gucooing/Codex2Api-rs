//! Wallet orders and price snapshots owned by a user's platform subscription.
use crate::coupons::{available_coupon, checkout_error};
use crate::{PlatformAccount, Result, Storage, StorageError, User, VirtualPlan};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, SqliteConnection};

const DAY_MS: i64 = 86_400_000;
const QUOTE_TTL_MS: i64 = 300_000;
const SELECT_ORDER: &str = "SELECT o.*,u.username,u.name AS user_name FROM subscription_orders o JOIN regular_users u ON u.id=o.user_id";

pub struct OrderRequest<'a> {
    pub user_id: &'a str,
    pub preview_token: &'a str,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CheckoutInput {
    pub plan_id: String,
    pub plan_revision: i64,
    pub subscription_revision: Option<i64>,
    pub payment_method: String,
    pub coupon_code: String,
}

#[derive(Deserialize, Serialize, PartialEq)]
struct CheckoutQuote {
    input: CheckoutInput,
    provider_id: String,
    plan_name: String,
    kind: String,
    subscription_id: Option<String>,
    previous_plan_id: Option<String>,
    previous_plan_name: Option<String>,
    unit_price_cents: i64,
    duration_days: i64,
    period_start_ms: i64,
    period_end_ms: i64,
    gross_cents: i64,
    credit_cents: i64,
    discount_cents: i64,
    amount_cents: i64,
    coupon_id: Option<String>,
    coupon_revision: Option<i64>,
    coupon_code: Option<String>,
    pricing: Value,
    quoted_at_ms: i64,
    expires_at_ms: i64,
}
impl CheckoutQuote {
    fn view(&self) -> Value {
        json!({"provider_id":self.provider_id,"plan_id":self.input.plan_id,"plan_name":self.plan_name,
            "kind":self.kind,"previous_plan_name":self.previous_plan_name,"unit_price_cents":self.unit_price_cents,
            "duration_days":self.duration_days,"period_start_ms":self.period_start_ms,"period_end_ms":self.period_end_ms,
            "gross_cents":self.gross_cents,"credit_cents":self.credit_cents,"discount_cents":self.discount_cents,
            "amount_cents":self.amount_cents,"coupon_code":self.coupon_code,"payment_method":self.input.payment_method,
            "pricing":self.pricing,"quoted_at_ms":self.quoted_at_ms,"expires_at_ms":self.expires_at_ms})
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OrderFilter {
    pub plan_id: String,
    pub status: String,
    pub search: String,
    pub page: i64,
    pub limit: i64,
}
impl Default for OrderFilter {
    fn default() -> Self {
        Self {
            plan_id: String::new(),
            status: String::new(),
            search: String::new(),
            page: 1,
            limit: 20,
        }
    }
}

#[derive(FromRow)]
pub struct SubscriptionOrder {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub user_name: String,
    pub request_id: String,
    pub request_signature: String,
    pub provider_id: String,
    pub plan_id: String,
    pub plan_name: String,
    pub plan_revision: i64,
    pub kind: String,
    pub status: String,
    pub subscription_id: Option<String>,
    pub subscription_revision: Option<i64>,
    pub previous_plan_id: Option<String>,
    pub previous_plan_name: Option<String>,
    pub unit_price_cents: i64,
    pub duration_days: i64,
    pub period_start_ms: i64,
    pub period_end_ms: i64,
    pub gross_cents: i64,
    pub credit_cents: i64,
    pub amount_cents: i64,
    pub discount_cents: i64,
    pub coupon_code: Option<String>,
    pub payment_method: String,
    pub pricing_json: String,
    pub created_at_ms: i64,
    pub quote_expires_at_ms: i64,
    pub paid_at_ms: Option<i64>,
    pub cancelled_at_ms: Option<i64>,
    pub cancel_reason: Option<String>,
    pub balance_cents: Option<i64>,
}
impl SubscriptionOrder {
    pub fn view(&self, admin: bool) -> Result<Value> {
        let status = if self.status == "pending"
            && self.quote_expires_at_ms <= Utc::now().timestamp_millis()
        {
            "expired"
        } else {
            &self.status
        };
        let mut value = json!({"id":self.id,"provider_id":self.provider_id,"plan_id":self.plan_id,"plan_name":self.plan_name,
            "kind":self.kind,"status":status,"previous_plan_name":self.previous_plan_name,
            "unit_price_cents":self.unit_price_cents,"duration_days":self.duration_days,
            "period_start_ms":self.period_start_ms,"period_end_ms":self.period_end_ms,
            "gross_cents":self.gross_cents,"credit_cents":self.credit_cents,"amount_cents":self.amount_cents,
            "discount_cents":self.discount_cents,"coupon_code":self.coupon_code,"payment_method":self.payment_method,
            "pricing":serde_json::from_str::<Value>(&self.pricing_json)?,"created_at_ms":self.created_at_ms,
            "quote_expires_at_ms":self.quote_expires_at_ms,"paid_at_ms":self.paid_at_ms,
            "cancelled_at_ms":self.cancelled_at_ms,"cancel_reason":self.cancel_reason,"balance_cents":self.balance_cents});
        if admin {
            value["user_id"] = json!(self.user_id);
            value["username"] = json!(self.username);
            value["user_name"] = json!(self.user_name);
        }
        Ok(value)
    }
}

#[derive(FromRow)]
struct PricePeriod {
    starts_at_ms: i64,
    ends_at_ms: i64,
    unit_price_cents: Option<i64>,
    duration_days: i64,
    credit_value_cents: Option<i64>,
    credit_duration_ms: i64,
}

fn invalid(message: &'static str) -> StorageError {
    checkout_error("invalid_order", message)
}
fn timestamp(value: &str) -> Result<i64> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.timestamp_millis())
        .map_err(|_| invalid("订阅到期时间无效"))
}
fn rfc3339(value: i64) -> Result<String> {
    DateTime::from_timestamp_millis(value)
        .map(|value| value.to_rfc3339())
        .ok_or_else(|| invalid("订阅期限超出范围"))
}
/// Round each invoice line half up to one USD cent, using integers throughout.
fn prorated_cents(price: i64, duration_days: i64, remaining_ms: i64) -> Result<i64> {
    if price < 0 || !(1..=3650).contains(&duration_days) || remaining_ms < 0 {
        return Err(invalid("套餐计价无效"));
    }
    rounded_fraction(price, duration_days * DAY_MS, remaining_ms)
}
fn rounded_fraction(value: i64, duration_ms: i64, remaining_ms: i64) -> Result<i64> {
    if value < 0 || duration_ms <= 0 || remaining_ms < 0 {
        return Err(invalid("套餐计价无效"));
    }
    let denominator = i128::from(duration_ms);
    let numerator = i128::from(value) * i128::from(remaining_ms);
    i64::try_from((numerator + denominator / 2) / denominator)
        .map_err(|_| invalid("订单金额超出范围"))
}

fn higher_daily_price(new_price: i64, new_days: i64, old_price: i64, old_days: i64) -> bool {
    i128::from(new_price) * i128::from(old_days) > i128::from(old_price) * i128::from(new_days)
}
async fn price_periods(
    connection: &mut SqliteConnection,
    subscription: &str,
    now: i64,
) -> Result<Vec<PricePeriod>> {
    Ok(sqlx::query_as("SELECT starts_at_ms,ends_at_ms,unit_price_cents,duration_days,credit_value_cents,credit_duration_ms FROM subscription_pricing_periods WHERE subscription_id=? AND superseded_at_ms IS NULL AND ends_at_ms>? ORDER BY starts_at_ms,id")
        .bind(subscription).bind(now).fetch_all(connection).await?)
}
fn remaining_credit(periods: &[PricePeriod], now: i64, end: i64) -> Result<(i64, Vec<Value>)> {
    let mut cursor = now;
    let mut total = 0_i64;
    let mut lines = Vec::new();
    for period in periods {
        if cursor >= end {
            break;
        }
        if period.starts_at_ms > cursor {
            return Err(invalid(
                "当前订阅缺少完整的售价快照，请联系管理员重新发放订阅",
            ));
        }
        let until = period.ends_at_ms.min(end);
        if until <= cursor {
            continue;
        }
        let price = period
            .unit_price_cents
            .ok_or_else(|| invalid("原套餐发放时未设置售价，无法计算升级抵扣"))?;
        let basis = period
            .credit_value_cents
            .ok_or_else(|| invalid("原订阅缺少抵扣基准，请联系管理员重新发放"))?;
        let credit = rounded_fraction(basis, period.credit_duration_ms, until - cursor)?;
        total = total
            .checked_add(credit)
            .ok_or_else(|| invalid("订单金额超出范围"))?;
        lines.push(json!({"starts_at_ms":cursor,"ends_at_ms":until,"unit_price_cents":price,"duration_days":period.duration_days,"credit_cents":credit,"credit_value_cents":basis,"credit_duration_ms":period.credit_duration_ms}));
        cursor = until;
    }
    if cursor < end {
        return Err(invalid(
            "当前订阅缺少完整的售价快照，请联系管理员重新发放订阅",
        ));
    }
    Ok((total, lines))
}

async fn find_order(
    connection: &mut SqliteConnection,
    owner: Option<&str>,
    id: &str,
) -> Result<Option<SubscriptionOrder>> {
    Ok(sqlx::query_as(&format!(
        "{SELECT_ORDER} WHERE o.id=? AND (? IS NULL OR o.user_id=?)"
    ))
    .bind(id)
    .bind(owner)
    .bind(owner)
    .fetch_optional(connection)
    .await?)
}

async fn quote_on(
    connection: &mut SqliteConnection,
    owner: &str,
    input: &CheckoutInput,
    now: i64,
    checked_at: i64,
) -> Result<CheckoutQuote> {
    if input.payment_method != "wallet" {
        return Err(checkout_error(
            "payment_method_unavailable",
            "所选支付方式不可用，请选择钱包余额",
        ));
    }
    let plan: VirtualPlan = sqlx::query_as("SELECT * FROM virtual_plans WHERE id=? AND revision=?")
        .bind(&input.plan_id)
        .bind(input.plan_revision)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| checkout_error("plan_changed", "套餐信息已变化，请重新预览"))?;
    if !plan.allow_purchase || plan.plan_type == "free" {
        return Err(checkout_error("plan_not_for_sale", "该套餐已关闭购买"));
    }
    let price = plan
        .sale_price_cents()?
        .ok_or_else(|| checkout_error("plan_unpriced", "该套餐暂未定价"))?;
    let days = plan.duration_days()?;
    let current =
        crate::user_subscriptions::subscription_account(&mut *connection, owner, &plan.provider_id)
            .await?;
    if current.as_ref().map(|(_, revision)| *revision) != input.subscription_revision {
        return Err(checkout_error(
            "subscription_changed",
            "订阅信息已变化，请重新预览",
        ));
    }
    let mut start = now;
    let mut end = now
        .checked_add(days * DAY_MS)
        .ok_or_else(|| invalid("订阅期限超出范围"))?;
    let mut kind = "purchase";
    let mut gross = price;
    let mut credit = 0_i64;
    let mut lines = Vec::new();
    if let Some((account, _)) = &current {
        if !account.enabled {
            return Err(invalid("该平台账户已停用，请联系管理员"));
        }
        let expiry = account
            .subscription_expires_at
            .as_deref()
            .map(timestamp)
            .transpose()?;
        if account.plan_type != "free" && expiry.is_none_or(|end| end > now) {
            let expiry = expiry.ok_or_else(|| invalid("长期有效订阅请联系管理员调整"))?;
            if account.plan_id == plan.id {
                kind = "renew";
                start = expiry;
                end = expiry
                    .checked_add(days * DAY_MS)
                    .ok_or_else(|| invalid("订阅期限超出范围"))?;
            } else {
                let periods = price_periods(&mut *connection, &account.id, now).await?;
                let active = periods
                    .iter()
                    .find(|p| p.starts_at_ms <= now && p.ends_at_ms > now)
                    .ok_or_else(|| invalid("当前订阅缺少售价快照，请联系管理员重新发放订阅"))?;
                let old_price = active
                    .unit_price_cents
                    .ok_or_else(|| invalid("原套餐发放时未设置售价，无法计算升级费用"))?;
                if !higher_daily_price(price, days, old_price, active.duration_days) {
                    return Err(invalid(
                        "当期仅允许升级到日均售价更高的套餐；降级或同价换套餐请等订阅到期",
                    ));
                }
                kind = "upgrade";
                end = expiry;
                gross = prorated_cents(price, days, expiry - now)?;
                (credit, lines) = remaining_credit(&periods, now, expiry)?;
            }
        }
    }
    rfc3339(end)?;
    let subtotal = gross.saturating_sub(credit).max(0);
    let previous_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM virtual_plans WHERE id=?")
            .bind(current.as_ref().map(|(a, _)| &a.plan_id))
            .fetch_optional(&mut *connection)
            .await?;

    if (kind == "upgrade" && end <= checked_at) || (kind == "renew" && start <= checked_at) {
        return Err(checkout_error(
            "subscription_changed",
            "原订阅已到期，请重新预览",
        ));
    }
    let coupon = available_coupon(
        connection,
        owner,
        &input.coupon_code,
        &plan.id,
        subtotal,
        checked_at,
    )
    .await?;
    let discount = coupon
        .as_ref()
        .map_or(0, |c| c.discount_cents.min(subtotal));
    let expires = coupon.as_ref().map_or(now + QUOTE_TTL_MS, |c| {
        (now + QUOTE_TTL_MS).min(c.ends_at_ms)
    });
    let expires = match kind {
        "renew" => expires.min(start),
        "upgrade" => expires.min(end),
        _ => expires,
    };
    Ok(CheckoutQuote {
        input: input.clone(),
        provider_id: plan.provider_id,
        plan_name: plan.name,
        kind: kind.into(),
        subscription_id: current.as_ref().map(|(a, _)| a.id.clone()),
        previous_plan_id: current.as_ref().map(|(a, _)| a.plan_id.clone()),
        previous_plan_name: previous_name,
        unit_price_cents: price,
        duration_days: days,
        period_start_ms: start,
        period_end_ms: end,
        gross_cents: gross,
        credit_cents: credit,
        discount_cents: discount,
        amount_cents: subtotal - discount,
        coupon_id: coupon.as_ref().map(|c| c.id.clone()),
        coupon_revision: coupon.as_ref().map(|c| c.revision),
        coupon_code: coupon.map(|c| c.code),
        pricing: json!({"quoted_at_ms":now,"basis":"daily_price","rounding":"half_up_cent_per_period","credit_periods":lines}),
        quoted_at_ms: now,
        expires_at_ms: expires,
    })
}

impl Storage {
    pub async fn subscription_order(
        &self,
        owner: Option<&str>,
        id: &str,
    ) -> Result<Option<SubscriptionOrder>> {
        find_order(&mut *self.pool().acquire().await?, owner, id).await
    }
    pub async fn admin_subscription_orders(
        &self,
        user: Option<&str>,
        filter: &OrderFilter,
    ) -> Result<Value> {
        self.orders_page(user, filter, true).await
    }
    pub async fn user_subscription_orders(
        &self,
        user: &str,
        filter: &OrderFilter,
    ) -> Result<Value> {
        self.orders_page(Some(user), filter, false).await
    }
    pub async fn order_plan_options(&self, owner: Option<&str>) -> Result<Vec<Value>> {
        let rows: Vec<(String,String,String)> = sqlx::query_as("SELECT plan_id,provider_id,plan_name FROM (SELECT plan_id,provider_id,plan_name,ROW_NUMBER() OVER(PARTITION BY plan_id ORDER BY created_at_ms DESC,id DESC) AS rank FROM subscription_orders WHERE (? IS NULL OR user_id=?)) WHERE rank=1 ORDER BY plan_name,plan_id")
            .bind(owner).bind(owner).fetch_all(self.pool()).await?;
        Ok(rows
            .into_iter()
            .map(|(id, provider_id, name)| json!({"id":id,"provider_id":provider_id,"name":name}))
            .collect())
    }
    async fn orders_page(
        &self,
        owner: Option<&str>,
        filter: &OrderFilter,
        admin: bool,
    ) -> Result<Value> {
        if filter.search.len() > 128
            || filter.plan_id.len() > 128
            || owner.is_some_and(|id| id.len() > 128)
            || filter.page < 1
            || !(1..=100).contains(&filter.limit)
            || !["", "pending", "paid", "cancelled", "expired"].contains(&filter.status.as_str())
        {
            return Err(invalid("订单筛选条件无效"));
        }
        let offset = (filter.page - 1)
            .checked_mul(filter.limit)
            .ok_or_else(|| invalid("订单页码超出范围"))?;
        let now = Utc::now().timestamp_millis();
        let search = filter.search.trim();
        let condition = " WHERE (? IS NULL OR o.user_id=?) AND (?='' OR o.plan_id=?) AND (?='' OR CASE WHEN o.status='pending' AND o.quote_expires_at_ms<=? THEN 'expired' ELSE o.status END=?) AND instr(lower(o.id),lower(?))>0";
        let total:i64=sqlx::query_scalar(&format!("SELECT COUNT(*) FROM subscription_orders o JOIN regular_users u ON u.id=o.user_id{condition}"))
            .bind(owner).bind(owner).bind(&filter.plan_id).bind(&filter.plan_id).bind(&filter.status).bind(now).bind(&filter.status).bind(search).fetch_one(self.pool()).await?;
        let orders: Vec<SubscriptionOrder> = sqlx::query_as(&format!(
            "{SELECT_ORDER}{condition} ORDER BY o.created_at_ms DESC,o.id DESC LIMIT ? OFFSET ?"
        ))
        .bind(owner)
        .bind(owner)
        .bind(&filter.plan_id)
        .bind(&filter.plan_id)
        .bind(&filter.status)
        .bind(now)
        .bind(&filter.status)
        .bind(search)
        .bind(filter.limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await?;
        let items = orders
            .iter()
            .map(|order| order.view(admin))
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({"items":items,"total":total,"page":filter.page,"limit":filter.limit}))
    }
    /// Read-only business preview: no order, reservation, entitlement or wallet write.
    pub async fn checkout_preview(&self, owner: &str, input: CheckoutInput) -> Result<Value> {
        let mut tx = self.pool().begin().await?;
        let balance: i64 =
            sqlx::query_scalar("SELECT wallet_cents FROM regular_users WHERE id=? AND enabled=1")
                .bind(owner)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StorageError::InvalidCredentials)?;
        let now = Utc::now().timestamp_millis();
        let quote = quote_on(&mut tx, owner, &input, now, now).await?;
        tx.commit().await?;
        let token = self.sign_jwt(crate::TokenPurpose::CheckoutPreview, json!({
            "sub":owner,"jti":uuid::Uuid::new_v4().to_string(),"iat":now/1000,"exp":quote.expires_at_ms/1000,
            "input":input,"quoted_at_ms":now,"expires_at_ms":quote.expires_at_ms,
            "quote_hash":crate::hash_token(&serde_json::to_string(&quote)?)
        })).await?;
        let mut view = quote.view();
        view["preview_token"] = token.into();
        view["wallet_balance_cents"] = balance.into();
        view["payment_methods"] = json!([{"id":"wallet","name":"钱包余额","currency":"USD"}]);
        Ok(view)
    }

    pub async fn create_subscription_order(
        &self,
        input: OrderRequest<'_>,
    ) -> Result<SubscriptionOrder> {
        let claims = self
            .verify_jwt(crate::TokenPurpose::CheckoutPreview, input.preview_token)
            .await
            .map_err(|error| match error {
                StorageError::InvalidJwt => {
                    checkout_error("preview_invalid", "预览已过期或无效，请重新预览后确认")
                }
                other => other,
            })?;
        if claims["sub"] != input.user_id {
            return Err(checkout_error(
                "preview_invalid",
                "该预览不属于当前用户，请重新预览",
            ));
        }
        let selection: CheckoutInput = serde_json::from_value(claims["input"].clone())?;
        let quoted_at = claims["quoted_at_ms"]
            .as_i64()
            .ok_or_else(|| invalid("预览时间无效"))?;
        let expires_at = claims["expires_at_ms"]
            .as_i64()
            .ok_or_else(|| invalid("预览时间无效"))?;
        let request_id = claims["jti"]
            .as_str()
            .ok_or_else(|| invalid("预览标识无效"))?;
        let signature = crate::hash_token(input.preview_token);
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let enabled: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM regular_users WHERE id=? AND enabled=1)",
        )
        .bind(input.user_id)
        .fetch_one(&mut *tx)
        .await?;
        if !enabled {
            return Err(StorageError::InvalidCredentials);
        }
        let previous: Option<(String, String)> = sqlx::query_as(
            "SELECT id,request_signature FROM subscription_orders WHERE user_id=? AND request_id=?",
        )
        .bind(input.user_id)
        .bind(request_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((id, prior_signature)) = previous {
            if signature != prior_signature {
                return Err(invalid("预览标识已用于其他订单"));
            }
            return find_order(&mut tx, Some(input.user_id), &id)
                .await?
                .ok_or_else(|| StorageError::AccountNotFound(id));
        }
        let now = Utc::now().timestamp_millis();
        if now >= expires_at {
            return Err(checkout_error("preview_expired", "预览已过期，请重新预览"));
        }
        let quote = quote_on(&mut tx, input.user_id, &selection, quoted_at, now).await?;
        if claims["quote_hash"] != crate::hash_token(&serde_json::to_string(&quote)?) {
            return Err(checkout_error(
                "preview_changed",
                "套餐、订阅或优惠券已变化，请重新预览后确认",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO subscription_orders(id,user_id,request_id,request_signature,provider_id,plan_id,plan_name,plan_revision,kind,subscription_id,subscription_revision,previous_plan_id,previous_plan_name,unit_price_cents,duration_days,period_start_ms,period_end_ms,gross_cents,credit_cents,subtotal_cents,pricing_json,created_at_ms,quote_expires_at_ms,discount_cents,coupon_id,coupon_code,payment_method) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(input.user_id).bind(request_id).bind(signature).bind(&quote.provider_id).bind(&quote.input.plan_id).bind(&quote.plan_name).bind(quote.input.plan_revision).bind(&quote.kind)
            .bind(&quote.subscription_id).bind(quote.input.subscription_revision).bind(&quote.previous_plan_id).bind(&quote.previous_plan_name)
            .bind(quote.unit_price_cents).bind(quote.duration_days).bind(quote.period_start_ms).bind(quote.period_end_ms).bind(quote.gross_cents).bind(quote.credit_cents).bind(quote.gross_cents.saturating_sub(quote.credit_cents).max(0))
            .bind(quote.pricing.to_string()).bind(now).bind(quote.expires_at_ms).bind(quote.discount_cents).bind(&quote.coupon_id).bind(&quote.coupon_code).bind(&quote.input.payment_method).execute(&mut *tx).await?;
        let order = find_order(&mut tx, Some(input.user_id), &id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(id))?;
        tx.commit().await?;
        Ok(order)
    }
    pub async fn pay_subscription_order(&self, owner: &str, id: &str) -> Result<SubscriptionOrder> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let user: User = sqlx::query_as("SELECT * FROM regular_users WHERE id=? AND enabled=1")
            .bind(owner)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StorageError::InvalidCredentials)?;
        let order = find_order(&mut tx, Some(owner), id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(id.into()))?;
        if order.status == "paid" {
            return Ok(order);
        }
        let now = Utc::now().timestamp_millis();
        if order.status != "pending" {
            return Err(checkout_error("order_cancelled", "订单已取消，无法支付"));
        }
        if order.quote_expires_at_ms <= now {
            return Err(checkout_error(
                "order_expired",
                "订单已过期，请重新预览并确认订单",
            ));
        }
        let plan: Option<VirtualPlan> = sqlx::query_as("SELECT * FROM virtual_plans WHERE id=?")
            .bind(&order.plan_id)
            .fetch_optional(&mut *tx)
            .await?;
        let current =
            crate::user_subscriptions::subscription_account(&mut tx, owner, &order.provider_id)
                .await?;
        let failure = if plan
            .as_ref()
            .is_none_or(|p| !p.allow_purchase || p.plan_type == "free")
        {
            Some(("plan_not_for_sale", "套餐已关闭购买，订单已取消"))
        } else if plan
            .as_ref()
            .is_some_and(|p| p.revision != order.plan_revision)
        {
            Some((
                "plan_changed",
                "套餐价格或权益已修改，订单已取消，请重新预览",
            ))
        } else if current.as_ref().is_some_and(|(a, _)| !a.enabled) {
            Some((
                "subscription_disabled",
                "该平台账户已停用，订单已取消，请联系管理员",
            ))
        } else if current.as_ref().map(|(_, r)| *r) != order.subscription_revision
            || current.as_ref().map(|(a, _)| a.id.as_str()) != order.subscription_id.as_deref()
        {
            Some((
                "subscription_changed",
                "订阅已被其他操作更新，订单已取消，请重新预览",
            ))
        } else if (order.kind == "upgrade" && order.period_end_ms <= now)
            || (order.kind == "renew" && order.period_start_ms <= now)
        {
            Some((
                "subscription_expired",
                "原订阅已到期，订单已取消，请重新选择套餐",
            ))
        } else {
            None
        };
        if let Some((code, message)) = failure {
            sqlx::query("UPDATE subscription_orders SET status='cancelled',cancelled_at_ms=?,cancel_reason=? WHERE id=? AND status='pending'")
                .bind(now).bind(message).bind(id).execute(&mut *tx).await?;
            tx.commit().await?;
            return Err(checkout_error(code, message));
        }
        let plan = plan.ok_or_else(|| checkout_error("plan_not_for_sale", "套餐不存在"))?;
        let balance:i64=sqlx::query_scalar("UPDATE users SET wallet_cents=wallet_cents-?,revision=revision+1 WHERE id=? AND wallet_cents>=? RETURNING wallet_cents")
            .bind(order.amount_cents).bind(owner).bind(order.amount_cents).fetch_optional(&mut *tx).await?.ok_or_else(||checkout_error("insufficient_balance", "钱包余额不足，订单保留待支付状态"))?;
        let (start, end) = if order.kind == "purchase" {
            (
                now,
                now.checked_add(order.duration_days * DAY_MS)
                    .ok_or_else(|| invalid("订阅期限超出范围"))?,
            )
        } else if order.kind == "upgrade" {
            (now, order.period_end_ms)
        } else {
            (order.period_start_ms, order.period_end_ms)
        };
        let expires = rfc3339(end)?;
        let origin = if order.kind == "upgrade" {
            "order_upgrade"
        } else {
            "order"
        };
        let subscription = crate::user_subscriptions::write_subscription(
            &mut tx,
            &user,
            &plan,
            current.as_ref().map(|(a, _)| a),
            Some(&expires),
            true,
            origin,
            None,
        )
        .await?;
        if order.kind != "renew" {
            supersede(&mut tx, &subscription, now).await?;
        }
        insert_period(
            &mut tx,
            &subscription,
            &plan,
            if order.kind == "upgrade" { now } else { start },
            end,
            "order",
            Some(&order.id),
            now,
            Some((
                order.gross_cents - order.discount_cents,
                if order.kind == "upgrade" {
                    order.period_end_ms - order.period_start_ms
                } else {
                    order.duration_days * DAY_MS
                },
            )),
        )
        .await?;
        sqlx::query("INSERT INTO wallet_entries(id,user_id,request_id,signature,plan_id,plan_revision,plan_name,provider_id,amount_cents,balance_cents,duration_days,expires_at,created_at,order_id,kind) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,'order_payment')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(owner).bind(format!("order:{}",order.id)).bind(&order.request_signature)
            .bind(&order.plan_id).bind(order.plan_revision).bind(&order.plan_name).bind(&order.provider_id).bind(-order.amount_cents).bind(balance)
            .bind(order.duration_days).bind(&expires).bind(rfc3339(now)?).bind(&order.id).execute(&mut *tx).await?;
        sqlx::query("UPDATE subscription_orders SET status='paid',paid_at_ms=?,balance_cents=?,period_start_ms=?,period_end_ms=? WHERE id=? AND status='pending'")
            .bind(now).bind(balance).bind(start).bind(end).bind(id).execute(&mut *tx).await?;
        let paid = find_order(&mut tx, Some(owner), id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(id.into()))?;
        tx.commit().await?;
        Ok(paid)
    }
    pub async fn cancel_subscription_order(
        &self,
        owner: Option<&str>,
        id: &str,
    ) -> Result<SubscriptionOrder> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let order = find_order(&mut tx, owner, id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(id.into()))?;
        if order.status == "paid" {
            return Err(invalid("已支付订单不能取消"));
        }
        if order.status == "pending" && order.quote_expires_at_ms > Utc::now().timestamp_millis() {
            sqlx::query("UPDATE subscription_orders SET status='cancelled',cancelled_at_ms=?,cancel_reason=? WHERE id=?")
                .bind(Utc::now().timestamp_millis()).bind(if owner.is_some(){"用户取消"}else{"管理员取消"}).bind(id).execute(&mut *tx).await?;
        }
        let result = find_order(&mut tx, owner, id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(id.into()))?;
        tx.commit().await?;
        Ok(result)
    }
}

async fn supersede(connection: &mut SqliteConnection, id: &str, now: i64) -> Result<()> {
    sqlx::query("UPDATE subscription_pricing_periods SET superseded_at_ms=? WHERE subscription_id=? AND superseded_at_ms IS NULL")
        .bind(now).bind(id).execute(connection).await?;
    Ok(())
}
#[allow(clippy::too_many_arguments)]
async fn insert_period(
    connection: &mut SqliteConnection,
    id: &str,
    plan: &VirtualPlan,
    start: i64,
    end: i64,
    source: &str,
    order: Option<&str>,
    now: i64,
    credit: Option<(i64, i64)>,
) -> Result<()> {
    if end <= start {
        return Ok(());
    }
    let value = credit.map(|c| c.0).or(plan.sale_price_cents()?);
    let duration = credit.map_or(plan.duration_days()? * DAY_MS, |c| c.1);
    sqlx::query("INSERT INTO subscription_pricing_periods(id,subscription_id,plan_id,starts_at_ms,ends_at_ms,unit_price_cents,duration_days,source,order_id,created_at_ms,credit_value_cents,credit_duration_ms) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(id).bind(&plan.id).bind(start).bind(end).bind(plan.sale_price_cents()?).bind(plan.duration_days()?).bind(source).bind(order).bind(now).bind(value).bind(duration).execute(connection).await?;
    Ok(())
}

pub(crate) async fn record_admin_pricing(
    connection: &mut SqliteConnection,
    id: &str,
    plan: &VirtualPlan,
    previous: Option<&PlatformAccount>,
    expires: Option<&str>,
) -> Result<()> {
    let now = Utc::now().timestamp_millis();
    let end = expires.map(timestamp).transpose()?;
    let old_end = previous
        .and_then(|a| a.subscription_expires_at.as_deref())
        .map(timestamp)
        .transpose()?;
    if previous.is_some_and(|a| a.plan_id == plan.id) && end == old_end {
        return Ok(());
    }
    if plan.plan_type == "free" || end.is_none_or(|end| end <= now) {
        supersede(connection, id, now).await?;
        return Ok(());
    }
    let end = end.ok_or_else(|| invalid("订阅到期时间无效"))?;
    if previous.is_some_and(|a| a.plan_id == plan.id)
        && let Some(old_end) = old_end.filter(|value| *value > now)
    {
        if end > old_end {
            return insert_period(connection, id, plan, old_end, end, "admin", None, now, None)
                .await;
        }
        sqlx::query("UPDATE subscription_pricing_periods SET superseded_at_ms=? WHERE subscription_id=? AND superseded_at_ms IS NULL AND starts_at_ms>=?").bind(now).bind(id).bind(end).execute(&mut *connection).await?;
        sqlx::query("UPDATE subscription_pricing_periods SET ends_at_ms=? WHERE subscription_id=? AND superseded_at_ms IS NULL AND ends_at_ms>?").bind(end).bind(id).bind(end).execute(connection).await?;
        return Ok(());
    }
    supersede(connection, id, now).await?;
    insert_period(connection, id, plan, now, end, "admin", None, now, None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upgrades_compare_daily_prices_and_round_only_invoice_lines() {
        assert!(higher_daily_price(6000, 30, 3000, 30));
        assert!(!higher_daily_price(6000, 60, 3000, 30));
        assert!(!higher_daily_price(1500, 30, 3000, 30));
        assert_eq!(prorated_cents(6000, 30, 15 * DAY_MS).unwrap(), 3000);
        assert_eq!(prorated_cents(3000, 30, 15 * DAY_MS).unwrap(), 1500);
        assert_eq!(prorated_cents(1, 30, 15 * DAY_MS).unwrap(), 1);
        let periods = vec![
            PricePeriod {
                starts_at_ms: 0,
                ends_at_ms: 30 * DAY_MS,
                unit_price_cents: Some(3000),
                duration_days: 30,
                credit_value_cents: Some(3000),
                credit_duration_ms: 30 * DAY_MS,
            },
            PricePeriod {
                starts_at_ms: 30 * DAY_MS,
                ends_at_ms: 60 * DAY_MS,
                unit_price_cents: Some(2400),
                duration_days: 30,
                credit_value_cents: Some(2400),
                credit_duration_ms: 30 * DAY_MS,
            },
        ];
        let (credit, lines) = remaining_credit(&periods, 15 * DAY_MS, 60 * DAY_MS).unwrap();
        assert_eq!(credit, 3900);
        assert_eq!(lines.len(), 2);
        assert_eq!(
            prorated_cents(6000, 30, 45 * DAY_MS).unwrap() - credit,
            5100
        );
        assert!(remaining_credit(&[], 0, DAY_MS).is_err());
    }
}

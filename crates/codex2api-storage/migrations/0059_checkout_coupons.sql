CREATE TABLE coupons (
    id TEXT PRIMARY KEY NOT NULL,
    code TEXT NOT NULL UNIQUE COLLATE NOCASE,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    discount_cents INTEGER NOT NULL CHECK(discount_cents>0),
    minimum_cents INTEGER NOT NULL CHECK(minimum_cents>=0),
    plan_id TEXT REFERENCES virtual_plans(id) ON DELETE RESTRICT,
    starts_at_ms INTEGER NOT NULL,
    ends_at_ms INTEGER NOT NULL CHECK(ends_at_ms>starts_at_ms),
    max_uses INTEGER CHECK(max_uses>0),
    per_user_limit INTEGER NOT NULL CHECK(per_user_limit>0),
    revision INTEGER NOT NULL DEFAULT 1,
    created_at_ms INTEGER NOT NULL
);
-- Preserve prior orders and their immutable paid amounts. The generated payable
-- amount is derived from the original subtotal and the confirmed coupon snapshot.
ALTER TABLE subscription_orders RENAME COLUMN amount_cents TO subtotal_cents;
ALTER TABLE subscription_orders ADD COLUMN discount_cents INTEGER NOT NULL DEFAULT 0 CHECK(discount_cents>=0 AND discount_cents<=subtotal_cents);
ALTER TABLE subscription_orders ADD COLUMN amount_cents INTEGER GENERATED ALWAYS AS (subtotal_cents-discount_cents) VIRTUAL;
ALTER TABLE subscription_orders ADD COLUMN payment_method TEXT NOT NULL DEFAULT 'wallet' CHECK(payment_method='wallet');
ALTER TABLE subscription_orders ADD COLUMN coupon_id TEXT REFERENCES coupons(id) ON DELETE RESTRICT;
ALTER TABLE subscription_orders ADD COLUMN coupon_code TEXT;
CREATE INDEX subscription_orders_coupon ON subscription_orders(coupon_id,user_id,status,quote_expires_at_ms);
ALTER TABLE subscription_pricing_periods ADD COLUMN credit_value_cents INTEGER CHECK(credit_value_cents>=0);
ALTER TABLE subscription_pricing_periods ADD COLUMN credit_duration_ms INTEGER NOT NULL DEFAULT 1 CHECK(credit_duration_ms>0);
UPDATE subscription_pricing_periods SET credit_value_cents=unit_price_cents,credit_duration_ms=duration_days*86400000;
CREATE TRIGGER subscription_credit_immutable BEFORE UPDATE OF credit_value_cents,credit_duration_ms ON subscription_pricing_periods
BEGIN SELECT RAISE(ABORT,'subscription credit snapshot is immutable'); END;

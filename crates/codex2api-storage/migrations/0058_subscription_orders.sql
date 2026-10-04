CREATE TABLE subscription_orders (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL,
    request_signature TEXT NOT NULL,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    plan_id TEXT NOT NULL,
    plan_name TEXT NOT NULL,
    plan_revision INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('purchase','renew','upgrade')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','paid','cancelled')),
    subscription_id TEXT,
    subscription_revision INTEGER,
    previous_plan_id TEXT,
    previous_plan_name TEXT,
    unit_price_cents INTEGER NOT NULL CHECK(unit_price_cents>=0),
    duration_days INTEGER NOT NULL CHECK(duration_days BETWEEN 1 AND 3650),
    period_start_ms INTEGER NOT NULL,
    period_end_ms INTEGER NOT NULL CHECK(period_end_ms>period_start_ms),
    gross_cents INTEGER NOT NULL CHECK(gross_cents>=0),
    credit_cents INTEGER NOT NULL CHECK(credit_cents>=0),
    amount_cents INTEGER NOT NULL CHECK(amount_cents>=0),
    pricing_json TEXT NOT NULL CHECK(json_valid(pricing_json)),
    created_at_ms INTEGER NOT NULL,
    quote_expires_at_ms INTEGER NOT NULL,
    paid_at_ms INTEGER,
    cancelled_at_ms INTEGER,
    cancel_reason TEXT,
    balance_cents INTEGER,
    UNIQUE(user_id,request_id),
    CHECK(amount_cents=MAX(0,gross_cents-credit_cents)),
    CHECK((status='paid')=(paid_at_ms IS NOT NULL AND balance_cents IS NOT NULL))
);
CREATE INDEX subscription_orders_owner_time ON subscription_orders(user_id,created_at_ms DESC,id);
CREATE INDEX subscription_orders_status_time ON subscription_orders(status,created_at_ms DESC,id);
CREATE TABLE subscription_pricing_periods (
    id TEXT PRIMARY KEY NOT NULL,
    subscription_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE RESTRICT,
    plan_id TEXT NOT NULL,
    starts_at_ms INTEGER NOT NULL,
    ends_at_ms INTEGER NOT NULL CHECK(ends_at_ms>starts_at_ms),
    unit_price_cents INTEGER CHECK(unit_price_cents>=0),
    duration_days INTEGER NOT NULL CHECK(duration_days BETWEEN 1 AND 3650),
    source TEXT NOT NULL CHECK(source IN ('admin','order')),
    order_id TEXT REFERENCES subscription_orders(id) ON DELETE RESTRICT,
    superseded_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    CHECK((source='order')=(order_id IS NOT NULL))
);
CREATE INDEX subscription_pricing_current ON subscription_pricing_periods(subscription_id,superseded_at_ms,starts_at_ms);
ALTER TABLE wallet_entries ADD COLUMN order_id TEXT REFERENCES subscription_orders(id) ON DELETE RESTRICT;
CREATE UNIQUE INDEX wallet_entry_order ON wallet_entries(order_id) WHERE order_id IS NOT NULL;
CREATE TRIGGER paid_order_immutable BEFORE UPDATE ON subscription_orders WHEN OLD.status='paid'
BEGIN SELECT RAISE(ABORT,'paid order is immutable'); END;
CREATE TRIGGER subscription_price_immutable BEFORE UPDATE OF unit_price_cents,duration_days,source,order_id ON subscription_pricing_periods
BEGIN SELECT RAISE(ABORT,'subscription price snapshot is immutable'); END;

CREATE TABLE wallet_entries_next (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('order_payment','system_adjustment')),
    plan_id TEXT,
    plan_revision INTEGER,
    plan_name TEXT,
    provider_id TEXT,
    amount_cents INTEGER NOT NULL CHECK(typeof(amount_cents)='integer'),
    balance_cents INTEGER NOT NULL CHECK(typeof(balance_cents)='integer' AND balance_cents>=0),
    balance_before_cents INTEGER GENERATED ALWAYS AS (balance_cents-amount_cents) VIRTUAL,
    duration_days INTEGER,
    expires_at TEXT,
    created_at TEXT NOT NULL,
    order_id TEXT REFERENCES subscription_orders(id) ON DELETE RESTRICT,
    operator_account_id TEXT REFERENCES accounts(id) ON DELETE RESTRICT,
    operator_name TEXT,
    reason TEXT,
    UNIQUE(user_id,request_id),
    CHECK(typeof(balance_before_cents)='integer' AND balance_before_cents>=0),
    CHECK((kind='order_payment' AND amount_cents<=0 AND plan_id IS NOT NULL AND plan_name IS NOT NULL AND operator_account_id IS NULL AND reason IS NULL)
       OR (kind='system_adjustment' AND amount_cents!=0 AND plan_id IS NULL AND order_id IS NULL AND operator_account_id IS NOT NULL AND operator_name IS NOT NULL AND (reason IS NULL OR length(trim(reason))>0)))
);
INSERT INTO wallet_entries_next(id,user_id,request_id,signature,kind,plan_id,plan_revision,plan_name,provider_id,amount_cents,balance_cents,duration_days,expires_at,created_at,order_id,operator_account_id,operator_name,reason)
SELECT id,user_id,request_id,signature,kind,plan_id,plan_revision,plan_name,provider_id,amount_cents,balance_cents,duration_days,expires_at,created_at,order_id,operator_account_id,operator_name,reason FROM wallet_entries;
DROP TABLE wallet_entries;
ALTER TABLE wallet_entries_next RENAME TO wallet_entries;
CREATE UNIQUE INDEX wallet_entry_order ON wallet_entries(order_id) WHERE order_id IS NOT NULL;
CREATE INDEX wallet_entries_owner_time ON wallet_entries(user_id,created_at DESC,id);
CREATE TRIGGER wallet_entry_immutable BEFORE UPDATE ON wallet_entries
BEGIN SELECT RAISE(ABORT,'wallet ledger entries are immutable'); END;
CREATE TRIGGER wallet_entry_no_delete BEFORE DELETE ON wallet_entries
BEGIN SELECT RAISE(ABORT,'wallet ledger entries cannot be deleted'); END;

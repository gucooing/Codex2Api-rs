CREATE TABLE api_groups (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    provider_id TEXT NOT NULL REFERENCES providers(id),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
    deleted INTEGER NOT NULL DEFAULT 0 CHECK(deleted IN (0,1)),
    allowed_models TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(allowed_models)),
    multiplier_micros INTEGER NOT NULL DEFAULT 1000000 CHECK(typeof(multiplier_micros)='integer' AND multiplier_micros BETWEEN 0 AND 1000000000),
    concurrency_limit INTEGER NOT NULL DEFAULT 0 CHECK(concurrency_limit BETWEEN 0 AND 1000000),
    rpm_limit INTEGER NOT NULL DEFAULT 0 CHECK(rpm_limit BETWEEN 0 AND 1000000),
    model_rules TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(model_rules)),
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE TABLE api_group_tags (
    group_id TEXT NOT NULL REFERENCES api_groups(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES supplier_tags(id) ON DELETE RESTRICT,
    PRIMARY KEY(group_id,tag_id)
);
CREATE TRIGGER api_group_provider_immutable BEFORE UPDATE OF provider_id ON api_groups
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'API group provider is immutable'); END;
CREATE TRIGGER api_group_tag_provider BEFORE INSERT ON api_group_tags
WHEN NOT EXISTS(SELECT 1 FROM api_groups g JOIN supplier_tags t ON t.provider_id=g.provider_id WHERE g.id=NEW.group_id AND t.id=NEW.tag_id)
BEGIN SELECT RAISE(ABORT,'API group tag provider mismatch'); END;
CREATE TRIGGER api_group_tag_immutable BEFORE UPDATE ON api_group_tags
BEGIN SELECT RAISE(ABORT,'API group tags must be replaced'); END;

CREATE TABLE api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    group_id TEXT NOT NULL REFERENCES api_groups(id) ON DELETE RESTRICT,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    key_prefix TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
    revoked_at TEXT,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revision INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX api_keys_owner ON api_keys(user_id,created_at DESC,id);
CREATE TRIGGER api_key_regular_user BEFORE INSERT ON api_keys
WHEN NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id AND enabled=1)
 OR NOT EXISTS(SELECT 1 FROM api_groups WHERE id=NEW.group_id AND enabled=1 AND deleted=0)
BEGIN SELECT RAISE(ABORT,'API keys require an enabled regular user and API group'); END;
CREATE TRIGGER api_key_identity_immutable BEFORE UPDATE ON api_keys
WHEN NEW.id!=OLD.id OR NEW.user_id!=OLD.user_id OR NEW.group_id!=OLD.group_id
 OR NEW.token_hash!=OLD.token_hash OR NEW.key_prefix!=OLD.key_prefix
 OR (OLD.revoked_at IS NOT NULL AND (NEW.revoked_at IS NULL OR NEW.revoked_at!=OLD.revoked_at OR NEW.enabled=1))
BEGIN SELECT RAISE(ABORT,'API key identity and revocation are immutable'); END;

CREATE TABLE api_wallet_state (
    user_id TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    pending_nano_usd INTEGER NOT NULL DEFAULT 0 CHECK(typeof(pending_nano_usd)='integer' AND pending_nano_usd>=0),
    last_usage_id TEXT REFERENCES api_usage_records(id) ON DELETE RESTRICT
);
CREATE TRIGGER api_wallet_regular_user BEFORE INSERT ON api_wallet_state
WHEN NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id)
BEGIN SELECT RAISE(ABORT,'API wallet requires a regular user'); END;
CREATE TRIGGER api_wallet_owner_immutable BEFORE UPDATE OF user_id ON api_wallet_state
WHEN NEW.user_id!=OLD.user_id
BEGIN SELECT RAISE(ABORT,'API wallet owner is immutable'); END;

CREATE TABLE api_usage_records (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    key_id TEXT NOT NULL REFERENCES api_keys(id) ON DELETE RESTRICT,
    group_id TEXT NOT NULL REFERENCES api_groups(id) ON DELETE RESTRICT,
    group_name TEXT NOT NULL,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    supplier_account_id TEXT,
    endpoint TEXT NOT NULL,
    model TEXT NOT NULL,
    actual_model TEXT,
    service_tier TEXT,
    multiplier_micros INTEGER NOT NULL CHECK(multiplier_micros BETWEEN 0 AND 1000000000),
    pricing_snapshot_json TEXT NOT NULL CHECK(json_valid(pricing_snapshot_json)),
    input_tokens INTEGER,
    output_tokens INTEGER,
    cached_tokens INTEGER,
    cache_write_tokens INTEGER,
    reasoning_tokens INTEGER,
    image_size TEXT,
    image_count INTEGER,
    image_usage_json TEXT,
    requested_at_ms INTEGER NOT NULL,
    completed_at_ms INTEGER,
    status TEXT NOT NULL DEFAULT 'started',
    http_status INTEGER,
    base_cost_nano_usd INTEGER,
    cost_nano_usd INTEGER,
    billing_status TEXT NOT NULL DEFAULT 'pending',
    wallet_debit_cents INTEGER NOT NULL DEFAULT 0 CHECK(wallet_debit_cents>=0),
    CHECK(cost_nano_usd IS NULL OR (typeof(cost_nano_usd)='integer' AND cost_nano_usd>=0))
);
CREATE INDEX api_usage_owner_time ON api_usage_records(user_id,requested_at_ms DESC,id);
CREATE INDEX api_usage_key_time ON api_usage_records(key_id,requested_at_ms DESC,id);
CREATE INDEX api_usage_group_time ON api_usage_records(group_id,requested_at_ms DESC,id);
CREATE INDEX api_usage_supplier_time ON api_usage_records(supplier_account_id,requested_at_ms);
CREATE TRIGGER api_usage_owner BEFORE INSERT ON api_usage_records
WHEN NOT EXISTS(SELECT 1 FROM api_keys k JOIN regular_users u ON u.id=k.user_id JOIN api_groups g ON g.id=k.group_id
 WHERE k.id=NEW.key_id AND u.id=NEW.user_id AND g.id=NEW.group_id AND g.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'API usage ownership mismatch'); END;
CREATE TRIGGER api_usage_identity_immutable BEFORE UPDATE ON api_usage_records
WHEN OLD.completed_at_ms IS NOT NULL OR NEW.id!=OLD.id OR NEW.user_id!=OLD.user_id OR NEW.key_id!=OLD.key_id
 OR NEW.group_id!=OLD.group_id OR NEW.provider_id!=OLD.provider_id OR NEW.model!=OLD.model
 OR NEW.multiplier_micros!=OLD.multiplier_micros OR NEW.pricing_snapshot_json!=OLD.pricing_snapshot_json
 OR NEW.requested_at_ms!=OLD.requested_at_ms OR NEW.endpoint!=OLD.endpoint OR NEW.group_name!=OLD.group_name
BEGIN SELECT RAISE(ABORT,'API usage identity and settled records are immutable'); END;
CREATE TRIGGER api_usage_no_delete BEFORE DELETE ON api_usage_records
BEGIN SELECT RAISE(ABORT,'API usage records cannot be deleted'); END;

CREATE TABLE wallet_entries_next (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('order_payment','system_adjustment','api_usage')),
    plan_id TEXT,
    plan_revision INTEGER,
    plan_name TEXT,
    provider_id TEXT,
    api_usage_record_id TEXT REFERENCES api_usage_records(id) ON DELETE RESTRICT,
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
       OR (kind='system_adjustment' AND amount_cents!=0 AND plan_id IS NULL AND order_id IS NULL AND operator_account_id IS NOT NULL AND operator_name IS NOT NULL AND (reason IS NULL OR length(trim(reason))>0))
       OR (kind='api_usage' AND amount_cents<0 AND plan_id IS NULL AND order_id IS NULL AND operator_account_id IS NULL AND operator_name IS NULL AND reason IS NULL AND api_usage_record_id IS NOT NULL))
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
CREATE TRIGGER regular_wallet_entry BEFORE INSERT ON wallet_entries
WHEN NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id)
BEGIN SELECT RAISE(ABORT,'wallet entries require a regular user'); END;
CREATE TRIGGER api_wallet_entry_owner BEFORE INSERT ON wallet_entries
WHEN NEW.kind='api_usage' AND NOT EXISTS(SELECT 1 FROM api_usage_records WHERE id=NEW.api_usage_record_id AND user_id=NEW.user_id)
BEGIN SELECT RAISE(ABORT,'API wallet entry ownership mismatch'); END;

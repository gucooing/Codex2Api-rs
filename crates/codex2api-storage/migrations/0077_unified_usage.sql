-- All generation traffic uses one ledger. Preserve frozen prices and wallet references.
CREATE TABLE usage_records_next (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL,
    account_name TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    subject_name TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    transport TEXT NOT NULL,
    model TEXT,
    reasoning_effort TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    cached_tokens INTEGER,
    cache_write_tokens INTEGER,
    reasoning_tokens INTEGER,
    image_size TEXT,
    first_byte_ms INTEGER,
    total_ms INTEGER,
    requested_at_ms INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'in_progress',
    http_status INTEGER,
    actual_model TEXT,
    service_tier TEXT,
    pricing_snapshot_json TEXT,
    cost_nano_usd INTEGER,
    billing_status TEXT NOT NULL DEFAULT 'legacy',
    billing_model TEXT,
    image_count INTEGER,
    image_usage_json TEXT,
    subject_kind TEXT NOT NULL DEFAULT 'virtual_account'
        CHECK(subject_kind IN ('virtual_account','api_key','retired_api_key')),
    provider_id TEXT NOT NULL DEFAULT 'chatgpt',
    error_code TEXT,
    error_message TEXT,
    upstream_request_id TEXT,
    quota_reset_credit_id TEXT,
    image_input_usage_json TEXT,
    billing_tier TEXT,
    failure_kind TEXT,
    failure_status INTEGER,
    user_id TEXT REFERENCES users(id) ON DELETE RESTRICT,
    key_id TEXT REFERENCES api_keys(id) ON DELETE RESTRICT,
    group_id TEXT REFERENCES api_groups(id) ON DELETE RESTRICT,
    group_name TEXT,
    wallet_user_id TEXT REFERENCES users(id) ON DELETE RESTRICT,
    multiplier_micros INTEGER NOT NULL DEFAULT 1000000 CHECK(typeof(multiplier_micros)='integer' AND multiplier_micros BETWEEN 0 AND 1000000000),
    base_cost_nano_usd INTEGER,
    wallet_debit_cents INTEGER NOT NULL DEFAULT 0 CHECK(wallet_debit_cents>=0),
    completed_at_ms INTEGER,
    CHECK(cost_nano_usd IS NULL OR (typeof(cost_nano_usd)='integer' AND cost_nano_usd>=0)),
    CHECK(wallet_user_id IS NULL OR (user_id IS NOT NULL AND wallet_user_id=user_id))
);
INSERT INTO usage_records_next(id,account_id,account_name,subject_id,subject_name,endpoint,transport,model,reasoning_effort,input_tokens,output_tokens,cached_tokens,cache_write_tokens,reasoning_tokens,image_size,first_byte_ms,total_ms,requested_at_ms,status,http_status,actual_model,service_tier,pricing_snapshot_json,cost_nano_usd,billing_status,billing_model,image_count,image_usage_json,subject_kind,provider_id,error_code,error_message,upstream_request_id,quota_reset_credit_id,image_input_usage_json,billing_tier,failure_kind,failure_status,user_id,base_cost_nano_usd,completed_at_ms)
SELECT r.id,r.account_id,r.account_name,r.subject_id,r.subject_name,r.endpoint,r.transport,r.model,r.reasoning_effort,r.input_tokens,r.output_tokens,r.cached_tokens,r.cache_write_tokens,r.reasoning_tokens,r.image_size,r.first_byte_ms,r.total_ms,r.requested_at_ms,r.status,r.http_status,r.actual_model,r.service_tier,r.pricing_snapshot_json,r.cost_nano_usd,r.billing_status,r.billing_model,r.image_count,r.image_usage_json,r.subject_kind,r.provider_id,r.error_code,r.error_message,r.upstream_request_id,r.quota_reset_credit_id,r.image_input_usage_json,r.billing_tier,r.failure_kind,r.failure_status,(SELECT user_id FROM regular_platforms WHERE id=r.subject_id AND r.subject_kind='virtual_account'),r.cost_nano_usd,CASE WHEN r.status!='in_progress' THEN r.requested_at_ms+r.total_ms END FROM usage_records r;
INSERT INTO usage_records_next(
    id,account_id,account_name,subject_id,subject_name,subject_kind,provider_id,
    user_id,key_id,group_id,group_name,wallet_user_id,endpoint,transport,model,actual_model,
    service_tier,multiplier_micros,pricing_snapshot_json,input_tokens,output_tokens,cached_tokens,
    cache_write_tokens,reasoning_tokens,image_size,image_count,image_usage_json,
    requested_at_ms,completed_at_ms,total_ms,status,http_status,base_cost_nano_usd,cost_nano_usd,
    billing_status,wallet_debit_cents)
SELECT r.id,COALESCE(r.supplier_account_id,''),COALESCE(s.display_name,s.email,''),r.key_id,
    u.username||' / '||k.name,'api_key',r.provider_id,r.user_id,r.key_id,r.group_id,r.group_name,
    r.user_id,r.endpoint,'http',r.model,r.actual_model,r.service_tier,r.multiplier_micros,
    r.pricing_snapshot_json,r.input_tokens,r.output_tokens,r.cached_tokens,r.cache_write_tokens,
    r.reasoning_tokens,r.image_size,r.image_count,r.image_usage_json,r.requested_at_ms,
    r.completed_at_ms,r.completed_at_ms-r.requested_at_ms,
    CASE r.status WHEN 'started' THEN 'in_progress' ELSE r.status END,r.http_status,
    r.base_cost_nano_usd,r.cost_nano_usd,r.billing_status,r.wallet_debit_cents
FROM api_usage_records r JOIN api_keys k ON k.id=r.key_id JOIN regular_users u ON u.id=r.user_id
LEFT JOIN supplier_accounts s ON s.id=r.supplier_account_id;
DROP TABLE usage_records;
ALTER TABLE usage_records_next RENAME TO usage_records;
CREATE TABLE wallet_entries_next (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('order_payment','system_adjustment','usage')),
    plan_id TEXT,
    plan_revision INTEGER,
    plan_name TEXT,
    provider_id TEXT,
    usage_record_id TEXT REFERENCES usage_records(id) ON DELETE RESTRICT,
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
       OR (kind='usage' AND amount_cents<0 AND plan_id IS NULL AND order_id IS NULL AND operator_account_id IS NULL AND operator_name IS NULL AND reason IS NULL AND usage_record_id IS NOT NULL))
);
INSERT INTO wallet_entries_next(id,user_id,request_id,signature,kind,plan_id,plan_revision,plan_name,provider_id,usage_record_id,amount_cents,balance_cents,duration_days,expires_at,created_at,order_id,operator_account_id,operator_name,reason)
SELECT id,user_id,request_id,signature,CASE kind WHEN 'api_usage' THEN 'usage' ELSE kind END,plan_id,plan_revision,plan_name,provider_id,api_usage_record_id,amount_cents,balance_cents,duration_days,expires_at,created_at,order_id,operator_account_id,operator_name,reason FROM wallet_entries;
DROP TABLE wallet_entries;
ALTER TABLE wallet_entries_next RENAME TO wallet_entries;
CREATE TABLE usage_wallet_state (
    user_id TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    pending_nano_usd INTEGER NOT NULL DEFAULT 0 CHECK(typeof(pending_nano_usd)='integer' AND pending_nano_usd>=0),
    last_usage_id TEXT REFERENCES usage_records(id) ON DELETE RESTRICT
);
INSERT INTO usage_wallet_state SELECT user_id,pending_nano_usd,last_usage_id FROM api_wallet_state;
DROP TABLE api_wallet_state;
DROP TABLE api_usage_records;
CREATE INDEX usage_records_time ON usage_records(requested_at_ms DESC,id DESC);
CREATE INDEX usage_records_account_time ON usage_records(account_id,requested_at_ms DESC);
CREATE INDEX usage_records_subject_time ON usage_records(subject_kind,subject_id,requested_at_ms DESC);
CREATE INDEX usage_records_model_time ON usage_records(model,requested_at_ms DESC);
CREATE INDEX usage_records_owner_time ON usage_records(user_id,requested_at_ms DESC,id);
CREATE INDEX usage_records_key_time ON usage_records(key_id,requested_at_ms DESC,id);
CREATE INDEX usage_records_group_time ON usage_records(group_id,requested_at_ms DESC,id);
CREATE INDEX usage_records_budget_window ON usage_records(subject_id,requested_at_ms,quota_reset_credit_id,cost_nano_usd);
CREATE INDEX usage_records_billing_totals ON usage_records(subject_id,cost_nano_usd,billing_status);
CREATE TRIGGER usage_owner BEFORE INSERT ON usage_records
WHEN (NEW.user_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id))
 OR (NEW.subject_kind='api_key' AND NOT EXISTS(SELECT 1 FROM api_keys k JOIN api_groups g ON g.id=k.group_id
     WHERE k.id=NEW.key_id AND k.id=NEW.subject_id AND k.user_id=NEW.user_id AND g.id=NEW.group_id AND g.provider_id=NEW.provider_id))
 OR (NEW.subject_kind='virtual_account' AND NEW.user_id IS NOT NULL AND NOT EXISTS(
     SELECT 1 FROM regular_platforms WHERE id=NEW.subject_id AND user_id=NEW.user_id AND provider_id=NEW.provider_id))
BEGIN SELECT RAISE(ABORT,'usage ownership mismatch'); END;
CREATE TRIGGER usage_identity_immutable BEFORE UPDATE ON usage_records
WHEN OLD.status!='in_progress' OR NEW.id!=OLD.id OR NEW.user_id IS NOT OLD.user_id
 OR NEW.subject_id!=OLD.subject_id OR NEW.subject_kind!=OLD.subject_kind OR NEW.key_id IS NOT OLD.key_id
 OR NEW.group_id IS NOT OLD.group_id OR NEW.group_name IS NOT OLD.group_name
 OR NEW.wallet_user_id IS NOT OLD.wallet_user_id OR NEW.provider_id!=OLD.provider_id OR NEW.model IS NOT OLD.model
 OR NEW.multiplier_micros!=OLD.multiplier_micros OR NEW.pricing_snapshot_json IS NOT OLD.pricing_snapshot_json
 OR NEW.requested_at_ms!=OLD.requested_at_ms OR NEW.endpoint!=OLD.endpoint
BEGIN SELECT RAISE(ABORT,'usage identity and settled records are immutable'); END;
CREATE TRIGGER usage_no_delete BEFORE DELETE ON usage_records
BEGIN SELECT RAISE(ABORT,'usage records cannot be deleted'); END;
CREATE UNIQUE INDEX wallet_entry_order ON wallet_entries(order_id) WHERE order_id IS NOT NULL;
CREATE INDEX wallet_entries_owner_time ON wallet_entries(user_id,created_at DESC,id);
CREATE TRIGGER wallet_entry_immutable BEFORE UPDATE ON wallet_entries
BEGIN SELECT RAISE(ABORT,'wallet ledger entries are immutable'); END;
CREATE TRIGGER wallet_entry_no_delete BEFORE DELETE ON wallet_entries
BEGIN SELECT RAISE(ABORT,'wallet ledger entries cannot be deleted'); END;
CREATE TRIGGER regular_wallet_entry BEFORE INSERT ON wallet_entries
WHEN NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id)
BEGIN SELECT RAISE(ABORT,'wallet entries require a regular user'); END;
CREATE TRIGGER usage_wallet_entry_owner BEFORE INSERT ON wallet_entries
WHEN NEW.kind='usage' AND NOT EXISTS(SELECT 1 FROM usage_records WHERE id=NEW.usage_record_id AND wallet_user_id=NEW.user_id)
BEGIN SELECT RAISE(ABORT,'usage wallet entry ownership mismatch'); END;
CREATE TRIGGER usage_wallet_regular_user BEFORE INSERT ON usage_wallet_state
WHEN NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id)
BEGIN SELECT RAISE(ABORT,'usage wallet requires a regular user'); END;
CREATE TRIGGER usage_wallet_owner_immutable BEFORE UPDATE OF user_id ON usage_wallet_state
WHEN NEW.user_id!=OLD.user_id
BEGIN SELECT RAISE(ABORT,'usage wallet owner is immutable'); END;

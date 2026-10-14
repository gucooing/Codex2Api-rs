-- Subscriptions belong to ordinary users; every platform can have multiple identities.

DROP TRIGGER "device_provider_registered";

DROP TRIGGER "device_provider_immutable";

DROP TRIGGER "authorization_provider_registered";

DROP TRIGGER "supplier_provider_registered";

DROP TRIGGER "supplier_provider_immutable";

DROP TRIGGER "supplier_workspace_identity_update";

DROP TRIGGER "supplier_auth_insert";

DROP TRIGGER "supplier_auth_update";

DROP TRIGGER "supplier_auth_delete";

DROP TRIGGER "supplier_tag_provider_insert";

DROP TRIGGER "supplier_tag_provider_immutable";

DROP TRIGGER "account_type_immutable";

DROP TRIGGER "admin_profile_type_insert";

DROP TRIGGER "admin_profile_owner_immutable";

DROP TRIGGER "user_profile_type_insert";

DROP TRIGGER "user_profile_owner_immutable";

DROP TRIGGER "paid_order_immutable";

DROP TRIGGER "subscription_price_immutable";

DROP TRIGGER "subscription_credit_immutable";

DROP TRIGGER "plan_provider_immutable";

DROP TRIGGER "preserve_free_plan_delete";

DROP TRIGGER "preserve_free_plan_type";

DROP TRIGGER "platform_free_plan_valid";

DROP TRIGGER "user_kind_immutable";

DROP TRIGGER "virtual_user_wallet_insert";

DROP TRIGGER "virtual_user_wallet_update";

DROP TRIGGER "regular_user_session";

DROP TRIGGER "user_session_owner_immutable";

DROP TRIGGER "virtual_user_platform";

DROP TRIGGER "regular_subscription_order";

DROP TRIGGER "subscription_order_owner_immutable";

DROP TRIGGER "platform_owner_immutable";

DROP TRIGGER "platform_plan_insert";

DROP TRIGGER "platform_plan_update";

DROP TRIGGER "platform_revision";

DROP TRIGGER "platform_disabled";

DROP TRIGGER "account_credentials_changed";

DROP TRIGGER "user_device_enabled";

DROP TRIGGER "user_authorization_enabled";

DROP TRIGGER "execution_route_provider_insert";

DROP TRIGGER "execution_route_provider_update";

DROP TRIGGER "api_group_provider_immutable";

DROP TRIGGER "api_group_tag_provider";

DROP TRIGGER "api_group_tag_immutable";

DROP TRIGGER "api_key_regular_user";

DROP TRIGGER "api_key_identity_immutable";

DROP TRIGGER "usage_owner";

DROP TRIGGER "usage_identity_immutable";

DROP TRIGGER "usage_no_delete";

DROP TRIGGER "wallet_entry_immutable";

DROP TRIGGER "wallet_entry_no_delete";

DROP TRIGGER "regular_wallet_entry";

DROP TRIGGER "usage_wallet_entry_owner";

DROP TRIGGER "usage_wallet_regular_user";

DROP TRIGGER "usage_wallet_owner_immutable";

DROP TRIGGER "usage_stream_immutable";

DROP TRIGGER "usage_source_name_immutable";

DROP VIEW "admin_identities";

DROP VIEW "user_identities";

DROP VIEW "platform_principals";

DROP VIEW "regular_users";

DROP VIEW "virtual_users";

DROP VIEW "regular_platforms";

DROP VIEW "virtual_platforms";

DROP VIEW "regular_subscription_pools";

DROP VIEW "effective_execution_routes";

CREATE TEMP TABLE removed_virtual_users AS SELECT id FROM users WHERE kind='virtual';

CREATE TEMP TABLE removed_virtual_platforms AS SELECT id FROM platform_accounts WHERE user_id IN (SELECT id FROM removed_virtual_users);

CREATE TEMP TABLE removed_subscriptions AS SELECT id FROM platform_accounts WHERE user_id IN (SELECT id FROM removed_virtual_users) OR plan_type='free';

DELETE FROM subscription_pricing_periods WHERE subscription_id IN (SELECT id FROM removed_virtual_platforms);

DELETE FROM usage_records WHERE subject_kind='virtual_account' AND (user_id IS NULL OR subject_id IN (SELECT id FROM removed_virtual_platforms));

CREATE TABLE subscription_pricing_periods_next (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    subscription_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    starts_at_ms INTEGER NOT NULL,
    ends_at_ms INTEGER NOT NULL CHECK(ends_at_ms>starts_at_ms),
    unit_price_cents INTEGER CHECK(unit_price_cents>=0),
    duration_days INTEGER NOT NULL CHECK(duration_days BETWEEN 1 AND 3650),
    source TEXT NOT NULL CHECK(source IN ('admin','order')),
    order_id TEXT REFERENCES subscription_orders(id) ON DELETE RESTRICT,
    superseded_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL, credit_value_cents INTEGER CHECK(credit_value_cents>=0), credit_duration_ms INTEGER NOT NULL DEFAULT 1 CHECK(credit_duration_ms>0),
    CHECK((source='order')=(order_id IS NOT NULL))
);

INSERT INTO subscription_pricing_periods_next(id,subscription_id,plan_id,starts_at_ms,ends_at_ms,unit_price_cents,duration_days,source,order_id,superseded_at_ms,created_at_ms,credit_value_cents,credit_duration_ms,user_id) SELECT p.id,p.subscription_id,p.plan_id,p.starts_at_ms,p.ends_at_ms,p.unit_price_cents,p.duration_days,p.source,p.order_id,p.superseded_at_ms,p.created_at_ms,p.credit_value_cents,p.credit_duration_ms,a.user_id FROM subscription_pricing_periods p JOIN platform_accounts a ON a.id=p.subscription_id;

DROP TABLE subscription_pricing_periods;

ALTER TABLE subscription_pricing_periods_next RENAME TO subscription_pricing_periods;

DELETE FROM virtual_access_tokens WHERE device_id IN (SELECT id FROM virtual_devices WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions));

DELETE FROM oauth_browser_flows WHERE id IN (SELECT flow_id FROM oauth_browser_identities WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions));

DELETE FROM virtual_remote_servers WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_reset_credits WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_reset_requests WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_reset_grants WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_device_authorizations WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_authorization_codes WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM oauth_browser_identities WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_client_state WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_resources WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_events WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_request_logs WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_conduits WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_rpm_limits WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_concurrency_limits WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM execution_routes WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM virtual_devices WHERE virtual_account_id IN (SELECT id FROM removed_subscriptions);

UPDATE subscription_orders SET status='cancelled',cancelled_at_ms=CAST(unixepoch('subsec')*1000 AS INTEGER),cancel_reason='订阅已移除' WHERE status='pending' AND subscription_id IN (SELECT id FROM removed_subscriptions);

DELETE FROM platform_accounts WHERE id IN (SELECT id FROM removed_subscriptions);

DELETE FROM users WHERE id IN (SELECT id FROM removed_virtual_users);

DELETE FROM accounts WHERE id IN (SELECT id FROM removed_virtual_users);

DROP TABLE platform_free_plans;

DELETE FROM coupons WHERE plan_id IN (SELECT id FROM virtual_plans WHERE plan_type='free');

DELETE FROM virtual_plans WHERE plan_type='free';

CREATE TABLE platform_accounts_next (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    plan_id TEXT NOT NULL REFERENCES virtual_plans(id) ON DELETE RESTRICT,
    plan_type TEXT NOT NULL,
    subscription_expires_at TEXT,
    subscription_started_at TEXT,
    quota_reset_credit_id TEXT,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
    created_at TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1
);

INSERT INTO platform_accounts_next SELECT * FROM platform_accounts;

DROP TABLE platform_accounts;

ALTER TABLE platform_accounts_next RENAME TO platform_accounts;

CREATE INDEX platform_accounts_user_provider ON platform_accounts(user_id,provider_id,id);

CREATE INDEX subscription_pricing_current ON subscription_pricing_periods(subscription_id,superseded_at_ms,starts_at_ms);

CREATE INDEX platform_accounts_plan ON platform_accounts(plan_id);

CREATE VIEW admin_identities AS
SELECT p.id,a.id AS account_id,a.username,a.password_hash,a.created_at,a.updated_at
FROM admin_users p JOIN accounts a ON a.id=p.account_id AND a.account_type='admin' AND a.enabled=1;

CREATE VIEW user_identities AS
SELECT u.id,a.username,a.password_hash,u.name,u.email,a.enabled,u.wallet_cents,u.revision,a.created_at,u.kind
FROM users u JOIN accounts a ON a.id=u.id AND a.account_type='user';

CREATE VIEW platform_principals AS
SELECT p.id,p.user_id,p.provider_id,a.username,a.password_hash,u.name,u.email,u.kind,
       p.plan_id,p.plan_type,p.subscription_expires_at,p.created_at,p.revision,
       (p.enabled=1 AND a.enabled=1) AS enabled
FROM platform_accounts p JOIN users u ON u.id=p.user_id
JOIN accounts a ON a.id=u.id AND a.account_type='user';

CREATE VIEW regular_users AS SELECT * FROM user_identities WHERE kind='regular';

CREATE VIEW regular_platforms AS SELECT * FROM platform_principals WHERE kind='regular';

CREATE VIEW regular_subscription_pools AS SELECT a.id AS virtual_account_id,a.provider_id,p.id AS plan_id,json_extract(p.config,'$.supplier_tag_id') AS tag_id FROM regular_platforms a JOIN virtual_plans p ON p.id=a.plan_id;

CREATE VIEW effective_execution_routes AS SELECT r.virtual_account_id,r.provider_id,r.revision,COALESCE(r.tag_id,p.tag_id) AS tag_id FROM execution_routes r JOIN regular_subscription_pools p ON p.virtual_account_id=r.virtual_account_id;

CREATE TRIGGER device_provider_registered BEFORE INSERT ON virtual_devices
WHEN NOT EXISTS(SELECT 1 FROM providers p WHERE p.id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'unknown session provider'); END;

CREATE TRIGGER device_provider_immutable BEFORE UPDATE OF provider_id ON virtual_devices
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'session provider is immutable'); END;

CREATE TRIGGER authorization_provider_registered BEFORE INSERT ON virtual_authorization_codes
WHEN NOT EXISTS(SELECT 1 FROM providers p WHERE p.id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'unknown authorization provider'); END;

CREATE TRIGGER supplier_provider_registered BEFORE INSERT ON supplier_accounts
WHEN NOT EXISTS(SELECT 1 FROM providers p WHERE p.id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'unknown supplier provider'); END;

CREATE TRIGGER supplier_provider_immutable BEFORE UPDATE OF provider_id ON supplier_accounts
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'supplier provider is immutable'); END;

CREATE TRIGGER supplier_workspace_identity_update AFTER UPDATE OF chatgpt_account_id,chatgpt_user_id ON supplier_accounts
WHEN NEW.chatgpt_account_id IS NOT OLD.chatgpt_account_id OR NEW.chatgpt_user_id IS NOT OLD.chatgpt_user_id
BEGIN UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=NEW.id; END;

CREATE TRIGGER supplier_auth_insert AFTER INSERT ON supplier_tokens
BEGIN UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=NEW.account_id; END;

CREATE TRIGGER supplier_auth_update AFTER UPDATE ON supplier_tokens
WHEN NEW.access_token IS NOT OLD.access_token OR NEW.id_token IS NOT OLD.id_token
  OR NEW.refresh_token IS NOT OLD.refresh_token OR NEW.raw_auth_json IS NOT OLD.raw_auth_json
BEGIN UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=NEW.account_id; END;

CREATE TRIGGER supplier_auth_delete AFTER DELETE ON supplier_tokens
BEGIN UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=OLD.account_id; END;

CREATE TRIGGER supplier_tag_provider_insert BEFORE INSERT ON supplier_tag_members
WHEN NOT EXISTS(SELECT 1 FROM supplier_tags t JOIN supplier_accounts a ON a.id=NEW.account_id WHERE t.id=NEW.tag_id AND t.provider_id=a.provider_id)
BEGIN SELECT RAISE(ABORT,'supplier tag provider mismatch'); END;

CREATE TRIGGER supplier_tag_provider_immutable BEFORE UPDATE OF provider_id ON supplier_tags
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'supplier tag provider is immutable'); END;

CREATE TRIGGER account_type_immutable BEFORE UPDATE OF id,account_type ON accounts
WHEN NEW.id!=OLD.id OR NEW.account_type!=OLD.account_type
BEGIN SELECT RAISE(ABORT,'account identity and type are immutable'); END;

CREATE TRIGGER admin_profile_type_insert BEFORE INSERT ON admin_users
WHEN NOT EXISTS(SELECT 1 FROM accounts WHERE id=NEW.account_id AND account_type='admin')
BEGIN SELECT RAISE(ABORT,'administrator profile requires administrator account'); END;

CREATE TRIGGER admin_profile_owner_immutable BEFORE UPDATE OF account_id ON admin_users
WHEN NEW.account_id!=OLD.account_id
BEGIN SELECT RAISE(ABORT,'administrator profile owner is immutable'); END;

CREATE TRIGGER user_profile_type_insert BEFORE INSERT ON users
WHEN NOT EXISTS(SELECT 1 FROM accounts WHERE id=NEW.id AND account_type='user')
BEGIN SELECT RAISE(ABORT,'user profile requires user account'); END;

CREATE TRIGGER user_profile_owner_immutable BEFORE UPDATE OF id ON users
WHEN NEW.id!=OLD.id
BEGIN SELECT RAISE(ABORT,'user profile owner is immutable'); END;

CREATE TRIGGER paid_order_immutable BEFORE UPDATE ON subscription_orders WHEN OLD.status='paid'
BEGIN SELECT RAISE(ABORT,'paid order is immutable'); END;

CREATE TRIGGER subscription_price_immutable BEFORE UPDATE OF unit_price_cents,duration_days,source,order_id ON subscription_pricing_periods
BEGIN SELECT RAISE(ABORT,'subscription price snapshot is immutable'); END;

CREATE TRIGGER subscription_credit_immutable BEFORE UPDATE OF credit_value_cents,credit_duration_ms ON subscription_pricing_periods
BEGIN SELECT RAISE(ABORT,'subscription credit snapshot is immutable'); END;

CREATE TRIGGER plan_provider_immutable BEFORE UPDATE OF provider_id ON virtual_plans
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'plan provider is immutable'); END;

CREATE TRIGGER user_kind_immutable BEFORE UPDATE OF kind ON users WHEN NEW.kind!=OLD.kind
BEGIN SELECT RAISE(ABORT,'user kind is immutable'); END;

CREATE TRIGGER regular_user_session BEFORE INSERT ON user_sessions
WHEN NOT EXISTS(SELECT 1 FROM user_identities WHERE id=NEW.user_id AND kind='regular' AND enabled=1)
BEGIN SELECT RAISE(ABORT,'user website requires a regular user'); END;

CREATE TRIGGER user_session_owner_immutable BEFORE UPDATE OF user_id ON user_sessions
WHEN NEW.user_id!=OLD.user_id
BEGIN SELECT RAISE(ABORT,'session owner is immutable'); END;

CREATE TRIGGER regular_subscription_order BEFORE INSERT ON subscription_orders
WHEN NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id)
BEGIN SELECT RAISE(ABORT,'orders require a regular user'); END;

CREATE TRIGGER subscription_order_owner_immutable BEFORE UPDATE OF user_id ON subscription_orders
WHEN NEW.user_id!=OLD.user_id
BEGIN SELECT RAISE(ABORT,'order owner is immutable'); END;

CREATE TRIGGER platform_owner_immutable BEFORE UPDATE OF id,user_id,provider_id ON platform_accounts
WHEN NEW.id!=OLD.id OR NEW.user_id!=OLD.user_id OR NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'platform identity and owner are immutable'); END;

CREATE TRIGGER platform_plan_insert BEFORE INSERT ON platform_accounts
WHEN NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id AND p.plan_type=NEW.plan_type)
BEGIN SELECT RAISE(ABORT,'platform plan mismatch'); END;

CREATE TRIGGER platform_plan_update BEFORE UPDATE OF plan_id,plan_type ON platform_accounts
WHEN NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id AND p.plan_type=NEW.plan_type)
BEGIN SELECT RAISE(ABORT,'platform plan mismatch'); END;

CREATE TRIGGER platform_revision AFTER UPDATE OF plan_id,plan_type,subscription_expires_at,enabled ON platform_accounts
BEGIN UPDATE platform_accounts SET revision=revision+1 WHERE id=NEW.id; END;

CREATE TRIGGER platform_disabled AFTER UPDATE OF enabled ON platform_accounts WHEN NEW.enabled=0
BEGIN
    DELETE FROM virtual_devices WHERE virtual_account_id=NEW.id;
    DELETE FROM virtual_authorization_codes WHERE virtual_account_id=NEW.id;
    DELETE FROM virtual_device_authorizations WHERE virtual_account_id=NEW.id;
    DELETE FROM oauth_browser_identities WHERE virtual_account_id=NEW.id;
END;

CREATE TRIGGER account_credentials_changed AFTER UPDATE OF password_hash,enabled ON accounts
WHEN NEW.password_hash!=OLD.password_hash OR NEW.enabled!=OLD.enabled
BEGIN
    DELETE FROM admin_sessions WHERE admin_user_id IN(SELECT id FROM admin_users WHERE account_id=NEW.id);
    DELETE FROM user_sessions WHERE user_id=NEW.id;
    DELETE FROM virtual_devices WHERE virtual_account_id IN(SELECT id FROM platform_accounts WHERE user_id=NEW.id);
    DELETE FROM virtual_authorization_codes WHERE virtual_account_id IN(SELECT id FROM platform_accounts WHERE user_id=NEW.id);
    DELETE FROM virtual_device_authorizations WHERE virtual_account_id IN(SELECT id FROM platform_accounts WHERE user_id=NEW.id);
    DELETE FROM oauth_browser_identities WHERE virtual_account_id IN(SELECT id FROM platform_accounts WHERE user_id=NEW.id);
    DELETE FROM oauth_browser_users WHERE user_id=NEW.id;
END;

CREATE TRIGGER user_device_enabled BEFORE INSERT ON virtual_devices
WHEN NOT EXISTS(SELECT 1 FROM platform_principals WHERE id=NEW.virtual_account_id AND enabled=1 AND provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'account unavailable'); END;

CREATE TRIGGER user_authorization_enabled BEFORE INSERT ON virtual_authorization_codes
WHEN NOT EXISTS(SELECT 1 FROM platform_principals WHERE id=NEW.virtual_account_id AND enabled=1 AND provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'account unavailable'); END;

CREATE TRIGGER execution_route_provider_insert BEFORE INSERT ON execution_routes
WHEN NOT EXISTS(SELECT 1 FROM platform_accounts p WHERE p.id=NEW.virtual_account_id AND p.provider_id=NEW.provider_id)
 OR (NEW.tag_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_tags WHERE id=NEW.tag_id AND provider_id=NEW.provider_id))
BEGIN SELECT RAISE(ABORT,'execution route pool mismatch'); END;

CREATE TRIGGER execution_route_provider_update BEFORE UPDATE ON execution_routes
WHEN NEW.virtual_account_id!=OLD.virtual_account_id OR NEW.provider_id!=OLD.provider_id
 OR (NEW.tag_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_tags WHERE id=NEW.tag_id AND provider_id=NEW.provider_id))
BEGIN SELECT RAISE(ABORT,'execution route pool mismatch'); END;

CREATE TRIGGER api_group_provider_immutable BEFORE UPDATE OF provider_id ON api_groups
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'API group provider is immutable'); END;

CREATE TRIGGER api_group_tag_provider BEFORE INSERT ON api_group_tags
WHEN NOT EXISTS(SELECT 1 FROM api_groups g JOIN supplier_tags t ON t.provider_id=g.provider_id WHERE g.id=NEW.group_id AND t.id=NEW.tag_id)
BEGIN SELECT RAISE(ABORT,'API group tag provider mismatch'); END;

CREATE TRIGGER api_group_tag_immutable BEFORE UPDATE ON api_group_tags
BEGIN SELECT RAISE(ABORT,'API group tags must be replaced'); END;

CREATE TRIGGER api_key_regular_user BEFORE INSERT ON api_keys
WHEN NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id AND enabled=1)
 OR NOT EXISTS(SELECT 1 FROM api_groups WHERE id=NEW.group_id AND enabled=1 AND deleted=0)
BEGIN SELECT RAISE(ABORT,'API keys require an enabled regular user and API group'); END;

CREATE TRIGGER api_key_identity_immutable BEFORE UPDATE ON api_keys
WHEN NEW.id!=OLD.id OR NEW.user_id!=OLD.user_id OR NEW.group_id!=OLD.group_id
 OR NEW.token_hash!=OLD.token_hash OR NEW.key_prefix!=OLD.key_prefix
 OR (OLD.revoked_at IS NOT NULL AND (NEW.revoked_at IS NULL OR NEW.revoked_at!=OLD.revoked_at OR NEW.enabled=1))
BEGIN SELECT RAISE(ABORT,'API key identity and revocation are immutable'); END;

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

CREATE TRIGGER usage_stream_immutable BEFORE UPDATE OF stream ON usage_records
WHEN NEW.stream IS NOT OLD.stream
BEGIN SELECT RAISE(ABORT,'usage response mode is immutable'); END;

CREATE TRIGGER usage_source_name_immutable
BEFORE UPDATE OF source_name ON usage_records
WHEN NEW.source_name IS NOT OLD.source_name
BEGIN SELECT RAISE(ABORT, 'usage source name is immutable'); END;

CREATE TRIGGER user_regular_only BEFORE INSERT ON users WHEN NEW.kind!='regular' BEGIN SELECT RAISE(ABORT,'only ordinary users are supported'); END;

CREATE TRIGGER subscription_price_owner_immutable BEFORE UPDATE OF user_id,subscription_id ON subscription_pricing_periods WHEN NEW.user_id!=OLD.user_id OR NEW.subscription_id!=OLD.subscription_id BEGIN SELECT RAISE(ABORT,'subscription history ownership is immutable'); END;

CREATE TABLE oauth_browser_users(flow_id TEXT PRIMARY KEY NOT NULL REFERENCES oauth_browser_flows(id) ON DELETE CASCADE,user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,password_hash TEXT NOT NULL,session_id TEXT REFERENCES user_sessions(id) ON DELETE CASCADE);

INSERT INTO oauth_browser_users(flow_id,user_id,password_hash,session_id) SELECT i.flow_id,a.user_id,i.password_hash,i.session_id FROM oauth_browser_identities i JOIN platform_accounts a ON a.id=i.virtual_account_id;

CREATE TRIGGER oauth_browser_subscription_owner BEFORE INSERT ON oauth_browser_identities WHEN NOT EXISTS(SELECT 1 FROM oauth_browser_users u JOIN platform_accounts a ON a.user_id=u.user_id WHERE u.flow_id=NEW.flow_id AND a.id=NEW.virtual_account_id) BEGIN SELECT RAISE(ABORT,'authorization subscription ownership mismatch'); END;

UPDATE meta SET value=json_remove(value,'$.default_rpm','$.default_concurrency') WHERE key='gateway_settings';

UPDATE virtual_plans SET config=json_set(config,'$.rpm_limit',40,'$.concurrency_limit',10);

DELETE FROM virtual_client_state WHERE state_key='subscription_policy';





DROP TABLE removed_subscriptions;

DROP TABLE removed_virtual_platforms;

DROP TABLE removed_virtual_users;

DELETE FROM admin_consumer_operations;
ALTER TABLE admin_consumer_operations RENAME TO admin_subscription_operations;
CREATE TRIGGER oauth_browser_subscription_owner_update BEFORE UPDATE ON oauth_browser_identities WHEN NOT EXISTS(SELECT 1 FROM oauth_browser_users u JOIN platform_accounts a ON a.user_id=u.user_id WHERE u.flow_id=NEW.flow_id AND a.id=NEW.virtual_account_id AND u.password_hash=NEW.password_hash) BEGIN SELECT RAISE(ABORT,'authorization subscription ownership mismatch'); END;

CREATE TRIGGER subscription_price_owner BEFORE INSERT ON subscription_pricing_periods WHEN NOT EXISTS(SELECT 1 FROM regular_platforms WHERE id=NEW.subscription_id AND user_id=NEW.user_id) BEGIN SELECT RAISE(ABORT,'subscription price ownership mismatch'); END;

-- All consumer credentials belong to users. Platform identities keep their IDs.
ALTER TABLE virtual_login_attempts RENAME TO user_login_attempts;
DROP VIEW virtual_principals;
DROP VIEW user_identities;
DROP TRIGGER account_credentials_changed;
DROP TRIGGER user_device_enabled;
DROP TRIGGER user_authorization_enabled;
DROP TRIGGER user_subscription_provider;
DROP TRIGGER user_subscription_owner_immutable;
DROP TRIGGER user_subscription_revision;

ALTER TABLE users ADD COLUMN kind TEXT NOT NULL DEFAULT 'regular' CHECK(kind IN ('regular','virtual'));

-- Never merge unrelated owners with the same login. Existing user logins win;
-- conflicting virtual logins receive a suffix, chosen against both namespaces.
CREATE TEMP TABLE migrating_users AS
SELECT v.id AS platform_id,lower(hex(randomblob(16))) AS user_id,v.username AS old_username,
       v.username AS username,v.password_hash,v.name,v.email,v.enabled,v.created_at
FROM virtual_accounts v WHERE NOT EXISTS(
    SELECT 1 FROM user_subscriptions s WHERE s.virtual_account_id=v.id
);
WITH RECURSIVE candidates(platform_id,n,username) AS (
    SELECT platform_id,0,old_username FROM migrating_users
    UNION ALL
    SELECT c.platform_id,c.n+1,substr(m.old_username,1,80)||'~virtual-'||m.user_id||
        CASE WHEN c.n=0 THEN '' ELSE '-'||c.n END
    FROM candidates c JOIN migrating_users m USING(platform_id)
    WHERE EXISTS(SELECT 1 FROM accounts a WHERE a.account_type='user' AND a.username=c.username COLLATE NOCASE)
       OR (c.n>0 AND EXISTS(SELECT 1 FROM migrating_users o WHERE o.platform_id!=c.platform_id AND o.old_username=c.username COLLATE NOCASE))
)
UPDATE migrating_users SET username=(
    SELECT c.username FROM candidates c WHERE c.platform_id=migrating_users.platform_id ORDER BY n DESC LIMIT 1
);
INSERT INTO accounts(id,account_type,username,password_hash,enabled,created_at,updated_at)
SELECT user_id,'user',username,password_hash,enabled,created_at,created_at FROM migrating_users;
INSERT INTO users(id,name,email,kind)
SELECT user_id,name,email,'virtual' FROM migrating_users;

-- Rename first so SQLite rewrites resource/device/history foreign keys.
ALTER TABLE virtual_accounts RENAME TO platform_accounts;
ALTER TABLE platform_accounts ADD COLUMN user_id TEXT REFERENCES users(id) ON DELETE RESTRICT;
ALTER TABLE platform_accounts ADD COLUMN revision INTEGER NOT NULL DEFAULT 1;
UPDATE platform_accounts SET user_id=COALESCE(
    (SELECT user_id FROM user_subscriptions s WHERE s.virtual_account_id=platform_accounts.id),
    (SELECT user_id FROM migrating_users m WHERE m.platform_id=platform_accounts.id)
),revision=COALESCE((SELECT revision FROM user_subscriptions s WHERE s.virtual_account_id=platform_accounts.id),1);
DROP TABLE user_subscriptions;
DROP TABLE migrating_users;

-- Rebuild only the parent; the migration connection has foreign keys disabled.
DROP TRIGGER consumer_plan_provider_insert;
DROP TRIGGER consumer_plan_provider_update;
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
    revision INTEGER NOT NULL DEFAULT 1,
    UNIQUE(user_id,provider_id)
);
INSERT INTO platform_accounts_next
SELECT id,user_id,provider_id,plan_id,plan_type,subscription_expires_at,subscription_started_at,
       quota_reset_credit_id,enabled,created_at,revision FROM platform_accounts;
-- External triggers must be removed during the parent rebuild.
DROP TRIGGER consumer_session_provider;
DROP TRIGGER consumer_authorization_provider;
DROP TRIGGER execution_route_provider_insert;
DROP TABLE platform_accounts;
ALTER TABLE platform_accounts_next RENAME TO platform_accounts;
CREATE INDEX platform_accounts_plan ON platform_accounts(plan_id);

CREATE VIEW user_identities AS
SELECT u.id,a.username,a.password_hash,u.name,u.email,a.enabled,u.wallet_cents,u.revision,a.created_at,u.kind
FROM users u JOIN accounts a ON a.id=u.id AND a.account_type='user';
CREATE VIEW platform_principals AS
SELECT p.id,p.user_id,p.provider_id,a.username,a.password_hash,u.name,u.email,u.kind,
       p.plan_id,p.plan_type,p.subscription_expires_at,p.created_at,p.revision,
       (p.enabled=1 AND a.enabled=1) AS enabled
FROM platform_accounts p JOIN users u ON u.id=p.user_id
JOIN accounts a ON a.id=u.id AND a.account_type='user';

-- Business readers choose a fixed scope, never the unfiltered identity view.
CREATE VIEW regular_users AS SELECT * FROM user_identities WHERE kind='regular';
CREATE VIEW virtual_users AS SELECT * FROM user_identities WHERE kind='virtual';
CREATE VIEW regular_platforms AS SELECT * FROM platform_principals WHERE kind='regular';
CREATE VIEW virtual_platforms AS SELECT * FROM platform_principals WHERE kind='virtual';

CREATE TRIGGER user_kind_immutable BEFORE UPDATE OF kind ON users WHEN NEW.kind!=OLD.kind
BEGIN SELECT RAISE(ABORT,'user kind is immutable'); END;
CREATE TRIGGER virtual_user_wallet_insert BEFORE INSERT ON users WHEN NEW.kind='virtual' AND NEW.wallet_cents!=0
BEGIN SELECT RAISE(ABORT,'virtual users cannot hold a wallet balance'); END;
CREATE TRIGGER virtual_user_wallet_update BEFORE UPDATE OF wallet_cents ON users WHEN NEW.kind='virtual' AND NEW.wallet_cents!=0
BEGIN SELECT RAISE(ABORT,'virtual users cannot hold a wallet balance'); END;
CREATE TRIGGER regular_user_session BEFORE INSERT ON user_sessions
WHEN NOT EXISTS(SELECT 1 FROM user_identities WHERE id=NEW.user_id AND kind='regular' AND enabled=1)
BEGIN SELECT RAISE(ABORT,'user website requires a regular user'); END;
CREATE TRIGGER user_session_owner_immutable BEFORE UPDATE OF user_id ON user_sessions
WHEN NEW.user_id!=OLD.user_id
BEGIN SELECT RAISE(ABORT,'session owner is immutable'); END;
CREATE TRIGGER virtual_user_platform BEFORE INSERT ON platform_accounts
WHEN EXISTS(SELECT 1 FROM users u JOIN platform_accounts p ON p.user_id=u.id WHERE u.id=NEW.user_id AND u.kind='virtual')
BEGIN SELECT RAISE(ABORT,'virtual users have one administered platform identity'); END;
CREATE TRIGGER regular_wallet_entry BEFORE INSERT ON wallet_entries
WHEN NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id)
BEGIN SELECT RAISE(ABORT,'wallet entries require a regular user'); END;
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
END;
CREATE TRIGGER user_device_enabled BEFORE INSERT ON virtual_devices
WHEN NOT EXISTS(SELECT 1 FROM platform_principals WHERE id=NEW.virtual_account_id AND enabled=1 AND provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'account unavailable'); END;
CREATE TRIGGER user_authorization_enabled BEFORE INSERT ON virtual_authorization_codes
WHEN NOT EXISTS(SELECT 1 FROM platform_principals WHERE id=NEW.virtual_account_id AND enabled=1 AND provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'account unavailable'); END;
CREATE TRIGGER execution_route_provider_insert BEFORE INSERT ON execution_routes
WHEN NOT EXISTS(SELECT 1 FROM platform_accounts p WHERE p.id=NEW.virtual_account_id AND p.provider_id=NEW.provider_id)
 OR (NEW.supplier_account_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_accounts s WHERE s.id=NEW.supplier_account_id AND s.provider_id=NEW.provider_id))
BEGIN SELECT RAISE(ABORT,'execution route provider mismatch'); END;

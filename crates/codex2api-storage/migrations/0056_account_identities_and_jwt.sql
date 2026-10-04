-- Login identities share one base table; role-specific data stays in profiles.
CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    account_type TEXT NOT NULL CHECK(account_type IN ('admin','user')),
    username TEXT NOT NULL COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(account_type,username)
);
INSERT INTO accounts(id,account_type,username,password_hash,enabled,created_at,updated_at)
SELECT lower(hex(randomblob(16))),'admin',username,password_hash,1,created_at,updated_at FROM admin_users;
INSERT INTO accounts(id,account_type,username,password_hash,enabled,created_at,updated_at)
SELECT id,'user',username,password_hash,enabled,created_at,created_at FROM users;

DROP TRIGGER user_credentials_changed;
DROP TRIGGER user_identity_changed;
DROP TRIGGER user_device_enabled;
DROP TRIGGER user_authorization_enabled;
CREATE TABLE admin_users_next (
    id INTEGER PRIMARY KEY CHECK(id=1),
    account_id TEXT NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE
);
INSERT INTO admin_users_next(id,account_id)
SELECT p.id,a.id FROM admin_users p JOIN accounts a ON a.account_type='admin' AND a.username=p.username;
DROP TABLE admin_users;
ALTER TABLE admin_users_next RENAME TO admin_users;
CREATE TABLE users_next (
    id TEXT PRIMARY KEY NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    wallet_cents INTEGER NOT NULL DEFAULT 0 CHECK(typeof(wallet_cents)='integer' AND wallet_cents>=0),
    revision INTEGER NOT NULL DEFAULT 1
);
INSERT INTO users_next(id,name,email,wallet_cents,revision)
SELECT id,name,email,wallet_cents,revision FROM users;
DROP TABLE users;
ALTER TABLE users_next RENAME TO users;

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

CREATE VIEW user_identities AS
SELECT u.id,a.username,a.password_hash,u.name,u.email,a.enabled,u.wallet_cents,u.revision,a.created_at
FROM users u JOIN accounts a ON a.id=u.id AND a.account_type='user';
CREATE VIEW admin_identities AS
SELECT p.id,a.id AS account_id,a.username,a.password_hash,a.created_at,a.updated_at
FROM admin_users p JOIN accounts a ON a.id=p.account_id AND a.account_type='admin' AND a.enabled=1;

-- Managed platform identities read their login/profile data through the owner.
-- Independent virtual accounts and all device/history identifiers stay intact.
UPDATE virtual_accounts SET password_hash='',name='',email=''
WHERE id IN(SELECT virtual_account_id FROM user_subscriptions);
CREATE VIEW virtual_principals AS
SELECT v.id,v.provider_id,
    CASE WHEN s.user_id IS NULL THEN v.username ELSE a.username END AS username,
    CASE WHEN s.user_id IS NULL THEN v.password_hash ELSE a.password_hash END AS password_hash,
    CASE WHEN s.user_id IS NULL THEN v.name ELSE u.name END AS name,
    CASE WHEN s.user_id IS NULL THEN v.email ELSE u.email END AS email,
    v.plan_id,v.plan_type,v.subscription_expires_at,v.created_at,
    (v.enabled=1 AND (s.user_id IS NULL OR (a.account_type='user' AND a.enabled=1))) AS enabled
FROM virtual_accounts v LEFT JOIN user_subscriptions s ON s.virtual_account_id=v.id
LEFT JOIN users u ON u.id=s.user_id LEFT JOIN accounts a ON a.id=u.id;

CREATE TRIGGER account_credentials_changed AFTER UPDATE OF password_hash,enabled ON accounts
WHEN NEW.password_hash!=OLD.password_hash OR NEW.enabled!=OLD.enabled
BEGIN
    DELETE FROM admin_sessions WHERE admin_user_id IN(SELECT id FROM admin_users WHERE account_id=NEW.id);
    DELETE FROM user_sessions WHERE user_id=NEW.id;
    DELETE FROM virtual_devices WHERE virtual_account_id IN(SELECT virtual_account_id FROM user_subscriptions WHERE user_id=NEW.id);
    DELETE FROM virtual_authorization_codes WHERE virtual_account_id IN(SELECT virtual_account_id FROM user_subscriptions WHERE user_id=NEW.id);
    DELETE FROM virtual_device_authorizations WHERE virtual_account_id IN(SELECT virtual_account_id FROM user_subscriptions WHERE user_id=NEW.id);
    DELETE FROM oauth_browser_identities WHERE virtual_account_id IN(SELECT virtual_account_id FROM user_subscriptions WHERE user_id=NEW.id);
END;
CREATE TRIGGER user_device_enabled BEFORE INSERT ON virtual_devices
WHEN NOT EXISTS(SELECT 1 FROM virtual_principals WHERE id=NEW.virtual_account_id AND enabled=1)
BEGIN SELECT RAISE(ABORT,'account unavailable'); END;

CREATE TABLE jwt_signing_keys (
    account_type TEXT PRIMARY KEY NOT NULL CHECK(account_type IN ('admin','user')),
    private_key TEXT NOT NULL UNIQUE
);
-- Web sessions adopt separate role-bound JWTs. Codex OAuth keys, completed
-- device authorizations and client refresh/access credentials remain intact.
DELETE FROM oauth_browser_identities;
DELETE FROM oauth_browser_flows;
DELETE FROM user_sessions;
DELETE FROM admin_sessions;
CREATE TRIGGER user_authorization_enabled BEFORE INSERT ON virtual_authorization_codes
WHEN NOT EXISTS(SELECT 1 FROM virtual_principals WHERE id=NEW.virtual_account_id AND enabled=1)
BEGIN SELECT RAISE(ABORT,'account unavailable'); END;

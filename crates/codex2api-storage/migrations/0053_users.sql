-- Independent users; existing standalone virtual accounts are deliberately untouched.
CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
    wallet_cents INTEGER NOT NULL DEFAULT 0 CHECK(typeof(wallet_cents)='integer' AND wallet_cents>=0),
    revision INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE TABLE user_sessions (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    csrf_token TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX user_sessions_owner ON user_sessions(user_id);
CREATE TABLE user_subscriptions (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    virtual_account_id TEXT NOT NULL UNIQUE REFERENCES virtual_accounts(id) ON DELETE RESTRICT,
    revision INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY(user_id,provider_id)
);
CREATE TRIGGER user_subscription_provider BEFORE INSERT ON user_subscriptions
WHEN NOT EXISTS(SELECT 1 FROM virtual_accounts WHERE id=NEW.virtual_account_id AND provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'subscription provider mismatch'); END;
CREATE TRIGGER user_subscription_owner_immutable BEFORE UPDATE OF user_id,provider_id,virtual_account_id ON user_subscriptions
BEGIN SELECT RAISE(ABORT,'subscription owner is immutable'); END;
CREATE TRIGGER user_subscription_revision AFTER UPDATE OF plan_id,plan_type,subscription_expires_at,enabled ON virtual_accounts
BEGIN UPDATE user_subscriptions SET revision=revision+1 WHERE virtual_account_id=NEW.id; END;
CREATE TABLE wallet_entries (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    plan_revision INTEGER NOT NULL,
    plan_name TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    amount_cents INTEGER NOT NULL CHECK(amount_cents<=0),
    balance_cents INTEGER NOT NULL CHECK(balance_cents>=0),
    duration_days INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(user_id,request_id)
);
CREATE TABLE oauth_browser_identities (
    flow_id TEXT PRIMARY KEY NOT NULL REFERENCES oauth_browser_flows(id) ON DELETE CASCADE,
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL,
    session_hash TEXT REFERENCES user_sessions(token_hash) ON DELETE CASCADE
);
CREATE TRIGGER user_credentials_changed AFTER UPDATE OF password_hash,enabled ON users
WHEN NEW.password_hash!=OLD.password_hash OR NEW.enabled!=OLD.enabled
BEGIN
    DELETE FROM user_sessions WHERE user_id=NEW.id;
    DELETE FROM virtual_devices WHERE virtual_account_id IN (SELECT virtual_account_id FROM user_subscriptions WHERE user_id=NEW.id);
    DELETE FROM virtual_authorization_codes WHERE virtual_account_id IN (SELECT virtual_account_id FROM user_subscriptions WHERE user_id=NEW.id);
    DELETE FROM virtual_device_authorizations WHERE virtual_account_id IN (SELECT virtual_account_id FROM user_subscriptions WHERE user_id=NEW.id);
    DELETE FROM oauth_browser_identities WHERE virtual_account_id IN (SELECT virtual_account_id FROM user_subscriptions WHERE user_id=NEW.id);
END;
CREATE TRIGGER user_identity_changed AFTER UPDATE OF password_hash,name,email ON users
BEGIN
    UPDATE virtual_accounts SET password_hash=NEW.password_hash,name=NEW.name,email=NEW.email
    WHERE id IN (SELECT virtual_account_id FROM user_subscriptions WHERE user_id=NEW.id);
END;
CREATE TRIGGER user_device_enabled BEFORE INSERT ON virtual_devices
WHEN EXISTS(SELECT 1 FROM user_subscriptions s JOIN users u ON u.id=s.user_id WHERE s.virtual_account_id=NEW.virtual_account_id AND u.enabled=0)
BEGIN SELECT RAISE(ABORT,'user disabled'); END;
CREATE TRIGGER user_authorization_enabled BEFORE INSERT ON virtual_authorization_codes
WHEN EXISTS(SELECT 1 FROM user_subscriptions s JOIN users u ON u.id=s.user_id WHERE s.virtual_account_id=NEW.virtual_account_id AND u.enabled=0)
BEGIN SELECT RAISE(ABORT,'user disabled'); END;

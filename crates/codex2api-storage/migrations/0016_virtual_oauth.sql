CREATE TABLE virtual_accounts (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    plan_type TEXT NOT NULL,
    subscription_expires_at TEXT,
    sync_quota INTEGER NOT NULL DEFAULT 1,
    primary_used_percent REAL NOT NULL DEFAULT 0 CHECK(primary_used_percent BETWEEN 0 AND 100),
    weekly_used_percent REAL NOT NULL DEFAULT 0 CHECK(weekly_used_percent BETWEEN 0 AND 100),
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE INDEX virtual_accounts_binding ON virtual_accounts(account_id);
CREATE TABLE virtual_devices (
    id TEXT PRIMARY KEY NOT NULL,
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    refresh_hash TEXT NOT NULL UNIQUE,
    installation_id TEXT,
    user_agent TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_login_at TEXT NOT NULL,
    last_used_at TEXT
);
CREATE INDEX virtual_devices_owner ON virtual_devices(virtual_account_id);
CREATE TABLE virtual_access_tokens (
    token_hash TEXT PRIMARY KEY NOT NULL,
    device_id TEXT NOT NULL REFERENCES virtual_devices(id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
);
CREATE INDEX virtual_access_expiry ON virtual_access_tokens(expires_at);
CREATE TABLE virtual_authorization_codes (
    code_hash TEXT PRIMARY KEY NOT NULL,
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE oauth_missing_endpoints (
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    hits INTEGER NOT NULL DEFAULT 1,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    PRIMARY KEY(method,path)
);
CREATE TABLE virtual_login_attempts (
    username_hash TEXT PRIMARY KEY NOT NULL,
    attempts INTEGER NOT NULL,
    window_start INTEGER NOT NULL
);
-- Remove retired manual RT credentials; historical usage is stored independently.
DROP TABLE oauth_authorization_codes;
DROP TABLE oauth_access_tokens;
DROP TABLE oauth_devices;
DROP TABLE oauth_credentials;

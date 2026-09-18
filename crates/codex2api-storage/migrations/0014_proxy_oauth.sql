CREATE TABLE oauth_credentials (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    refresh_hash TEXT NOT NULL UNIQUE,
    refresh_token TEXT NOT NULL,
    token_prefix TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    paused_at TEXT
);
CREATE INDEX oauth_credentials_account ON oauth_credentials(account_id);

CREATE TABLE oauth_devices (
    credential_id TEXT NOT NULL REFERENCES oauth_credentials(id) ON DELETE CASCADE,
    device_key TEXT NOT NULL,
    installation_id TEXT,
    user_agent TEXT NOT NULL,
    first_login_at TEXT NOT NULL,
    last_login_at TEXT NOT NULL,
    last_used_at TEXT,
    PRIMARY KEY (credential_id, device_key)
);

CREATE TABLE oauth_access_tokens (
    token_hash TEXT PRIMARY KEY NOT NULL,
    credential_id TEXT NOT NULL REFERENCES oauth_credentials(id) ON DELETE CASCADE,
    device_key TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX oauth_access_credential ON oauth_access_tokens(credential_id);
CREATE INDEX oauth_access_expiry ON oauth_access_tokens(expires_at);

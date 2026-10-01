CREATE TABLE virtual_device_authorizations (
    id_hash TEXT PRIMARY KEY,
    user_code_hash TEXT NOT NULL UNIQUE,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    code_verifier TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    last_poll_at INTEGER NOT NULL DEFAULT 0,
    authorization_code TEXT,
    virtual_account_id TEXT REFERENCES virtual_accounts(id) ON DELETE CASCADE
);
CREATE INDEX virtual_device_authorizations_expiry ON virtual_device_authorizations(expires_at);

CREATE TABLE virtual_conduits (
    token_hash TEXT PRIMARY KEY NOT NULL,
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    upstream_account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    upstream_token TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);

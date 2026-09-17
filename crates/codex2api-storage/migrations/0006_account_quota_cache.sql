CREATE TABLE account_quota_cache (
    account_id TEXT PRIMARY KEY NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    response_json TEXT NOT NULL,
    observed_at TEXT NOT NULL
);

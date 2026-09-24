CREATE TABLE desktop_diagnostics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_hash TEXT NOT NULL UNIQUE,
    source TEXT NOT NULL,
    virtual_account_id TEXT REFERENCES virtual_accounts(id) ON DELETE SET NULL,
    record_count INTEGER NOT NULL,
    summaries_json TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 1,
    first_seen_at_ms INTEGER NOT NULL,
    last_seen_at_ms INTEGER NOT NULL
);
CREATE TABLE desktop_public_resources (
    path TEXT PRIMARY KEY,
    content BLOB NOT NULL,
    headers_json TEXT NOT NULL,
    fetched_at_ms INTEGER NOT NULL
);

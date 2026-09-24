CREATE TABLE virtual_resources (
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    upstream_account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
    value_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (virtual_account_id, kind, id)
);
CREATE TABLE virtual_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    topic TEXT NOT NULL,
    value_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX virtual_events_owner ON virtual_events(virtual_account_id,id);
CREATE TABLE virtual_request_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    status INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    created_at_ms INTEGER NOT NULL
);
CREATE INDEX virtual_logs_owner ON virtual_request_logs(virtual_account_id,id);

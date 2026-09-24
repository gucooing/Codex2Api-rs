-- Communication health is independent from administrator enable/disable intent.
CREATE TABLE supplier_health (
    account_id TEXT PRIMARY KEY REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    error_message TEXT,
    error_at TEXT,
    revision INTEGER NOT NULL DEFAULT 0
);

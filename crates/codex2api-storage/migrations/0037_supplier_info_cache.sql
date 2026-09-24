CREATE TABLE supplier_info_cache (
    account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    section TEXT NOT NULL CHECK (section IN ('usage', 'details', 'credits')),
    response_json TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    PRIMARY KEY (account_id, section)
);

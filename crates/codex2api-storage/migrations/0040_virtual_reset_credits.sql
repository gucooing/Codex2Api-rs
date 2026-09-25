-- Service-issued reset cards, separate from supplier credits and the usage ledger.
CREATE TABLE virtual_reset_grants (
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    request_id TEXT NOT NULL,
    quantity INTEGER NOT NULL CHECK(quantity BETWEEN 1 AND 100),
    note TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    available_at_ms INTEGER NOT NULL,
    expires_at_ms INTEGER,
    PRIMARY KEY(virtual_account_id, request_id)
);
CREATE TABLE virtual_reset_credits (
    id TEXT PRIMARY KEY,
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    grant_request_id TEXT NOT NULL,
    granted_at_ms INTEGER NOT NULL,
    redeemed_at_ms INTEGER,
    redeemed_by TEXT,
    windows_reset INTEGER NOT NULL DEFAULT 0,
    available_at_ms INTEGER NOT NULL DEFAULT 0,
    expires_at_ms INTEGER,
    source TEXT NOT NULL DEFAULT 'card',
    FOREIGN KEY(virtual_account_id, grant_request_id)
        REFERENCES virtual_reset_grants(virtual_account_id, request_id)
);
CREATE INDEX virtual_reset_credits_owner ON virtual_reset_credits(virtual_account_id, redeemed_at_ms, granted_at_ms);
CREATE TABLE virtual_reset_requests (
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    request_id TEXT NOT NULL,
    response_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY(virtual_account_id, request_id)
);
-- Stamp the reset generation at request insertion, including requests sharing a
-- millisecond with redemption. Late settlement cannot re-enter the new quota.
ALTER TABLE virtual_accounts ADD COLUMN quota_reset_credit_id TEXT;
ALTER TABLE usage_records ADD COLUMN quota_reset_credit_id TEXT;

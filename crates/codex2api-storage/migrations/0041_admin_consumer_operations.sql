-- Durable idempotency for administrator operations, including deleted accounts.
CREATE TABLE admin_consumer_operations (
    scope TEXT NOT NULL,
    request_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    PRIMARY KEY(scope, request_id)
);
CREATE INDEX virtual_accounts_list ON virtual_accounts(id, enabled, plan_type);

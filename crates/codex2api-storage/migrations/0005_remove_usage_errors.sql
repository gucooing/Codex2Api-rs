-- Remove the reverted feature whether or not its column was manually dropped.
-- SQLx executes the complete migration in a transaction.
-- Migration history can remain after manual removal of the usage table.
CREATE TABLE IF NOT EXISTS usage_records (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL,
    account_name TEXT NOT NULL,
    api_key_id TEXT NOT NULL,
    api_key_name TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    transport TEXT NOT NULL,
    model TEXT,
    reasoning_effort TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    cached_tokens INTEGER,
    cache_write_tokens INTEGER,
    reasoning_tokens INTEGER,
    image_size TEXT,
    first_byte_ms INTEGER,
    total_ms INTEGER,
    requested_at_ms INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'in_progress',
    http_status INTEGER
);

CREATE TABLE usage_records_without_errors (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL,
    account_name TEXT NOT NULL,
    api_key_id TEXT NOT NULL,
    api_key_name TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    transport TEXT NOT NULL,
    model TEXT,
    reasoning_effort TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    cached_tokens INTEGER,
    cache_write_tokens INTEGER,
    reasoning_tokens INTEGER,
    image_size TEXT,
    first_byte_ms INTEGER,
    total_ms INTEGER,
    requested_at_ms INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'in_progress',
    http_status INTEGER
);

INSERT INTO usage_records_without_errors (
    id, account_id, account_name, api_key_id, api_key_name, endpoint, transport,
    model, reasoning_effort, input_tokens, output_tokens, cached_tokens,
    cache_write_tokens, reasoning_tokens, image_size, first_byte_ms, total_ms,
    requested_at_ms, status, http_status
)
SELECT
    id, account_id, account_name, api_key_id, api_key_name, endpoint, transport,
    model, reasoning_effort, input_tokens, output_tokens, cached_tokens,
    cache_write_tokens, reasoning_tokens, image_size, first_byte_ms, total_ms,
    requested_at_ms, status, http_status
FROM usage_records;

DROP TABLE usage_records;
ALTER TABLE usage_records_without_errors RENAME TO usage_records;

CREATE INDEX usage_records_time ON usage_records(requested_at_ms DESC, id DESC);
CREATE INDEX usage_records_account_time ON usage_records(account_id, requested_at_ms DESC);
CREATE INDEX usage_records_key_time ON usage_records(api_key_id, requested_at_ms DESC);
CREATE INDEX usage_records_model_time ON usage_records(model, requested_at_ms DESC);

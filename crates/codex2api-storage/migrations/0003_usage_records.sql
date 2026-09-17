-- Snapshot labels survive account/key renaming or removal. Never store bearer keys or prompts.
CREATE TABLE usage_records (
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
CREATE INDEX usage_records_time ON usage_records(requested_at_ms DESC, id DESC);
CREATE INDEX usage_records_account_time ON usage_records(account_id, requested_at_ms DESC);
CREATE INDEX usage_records_key_time ON usage_records(api_key_id, requested_at_ms DESC);
CREATE INDEX usage_records_model_time ON usage_records(model, requested_at_ms DESC);

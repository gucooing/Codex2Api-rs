-- Routing caches are disposable. Keep configuration, replace the old state machine.
DROP TABLE turn_state_cache;
CREATE TABLE turn_state_cache (
    account_id TEXT NOT NULL REFERENCES turn_state_settings(account_id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    owner TEXT NOT NULL,
    revision TEXT NOT NULL,
    token TEXT,
    source TEXT NOT NULL DEFAULT '',
    captured_at INTEGER NOT NULL DEFAULT 0,
    issued_at INTEGER NOT NULL DEFAULT 0,
    expires_at INTEGER NOT NULL DEFAULT 0,
    injections INTEGER NOT NULL DEFAULT 0,
    request_count INTEGER NOT NULL DEFAULT 0,
    response_count INTEGER NOT NULL DEFAULT 0,
    request_captures INTEGER NOT NULL DEFAULT 0,
    response_captures INTEGER NOT NULL DEFAULT 0,
    last_request_at INTEGER NOT NULL DEFAULT 0,
    request_result TEXT NOT NULL DEFAULT 'unseen',
    request_length INTEGER NOT NULL DEFAULT 0,
    request_blocks INTEGER NOT NULL DEFAULT 0,
    last_response_at INTEGER NOT NULL DEFAULT 0,
    response_status INTEGER NOT NULL DEFAULT 0,
    response_result TEXT NOT NULL DEFAULT 'unseen',
    response_length INTEGER NOT NULL DEFAULT 0,
    response_blocks INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(account_id, model)
);

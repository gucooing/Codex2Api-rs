CREATE TABLE virtual_remote_servers (
    id TEXT PRIMARY KEY,
    environment_id TEXT NOT NULL UNIQUE,
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL REFERENCES virtual_devices(id) ON DELETE CASCADE,
    installation_id TEXT NOT NULL,
    name TEXT NOT NULL,
    os TEXT NOT NULL,
    arch TEXT NOT NULL,
    app_server_version TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at INTEGER NOT NULL,
    connection_id TEXT,
    connected_until_ms INTEGER NOT NULL DEFAULT 0,
    created_at_ms INTEGER NOT NULL,
    last_seen_at_ms INTEGER NOT NULL,
    UNIQUE(virtual_account_id, installation_id)
);

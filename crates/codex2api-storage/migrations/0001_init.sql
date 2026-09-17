-- SQLite schema for Codex2API.
-- Implementation aligned with official Codex commit a8964cb1bad67bc26a826fb07d1bef99c6a3f008.

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

INSERT OR IGNORE INTO meta (key, value) VALUES
    ('codex_ref_commit', 'a8964cb1bad67bc26a826fb07d1bef99c6a3f008'),
    ('codex_ref_repo', 'https://github.com/openai/codex'),
    ('schema_version', '1');

CREATE TABLE IF NOT EXISTS admin_users (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS admin_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    admin_user_id INTEGER NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL,
    display_name TEXT,
    chatgpt_account_id TEXT,
    chatgpt_user_id TEXT,
    email TEXT,
    plan_type TEXT,
    installation_id TEXT NOT NULL,
    originator TEXT NOT NULL,
    user_agent TEXT NOT NULL,
    os_type TEXT NOT NULL,
    os_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    home_dir TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_used_at TEXT,
    UNIQUE (chatgpt_account_id)
);

CREATE TABLE IF NOT EXISTS account_tokens (
    account_id TEXT PRIMARY KEY NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    auth_mode TEXT,
    id_token TEXT,
    access_token TEXT,
    refresh_token TEXT,
    last_refresh TEXT,
    raw_auth_json TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS oauth_pending (
    state TEXT PRIMARY KEY NOT NULL,
    code_verifier TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    account_id TEXT,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS proxy_api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name TEXT,
    key_hash TEXT NOT NULL UNIQUE,
    key_prefix TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT
);

CREATE TABLE IF NOT EXISTS account_runtime (
    account_id TEXT PRIMARY KEY NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    session_id TEXT,
    extra_json TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_proxy_api_keys_account ON proxy_api_keys(account_id);
CREATE INDEX IF NOT EXISTS idx_admin_sessions_expires ON admin_sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_oauth_pending_expires ON oauth_pending(expires_at);

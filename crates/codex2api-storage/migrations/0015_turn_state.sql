CREATE TABLE turn_state_settings (
    account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    revision TEXT NOT NULL,
    config TEXT NOT NULL
);
CREATE TABLE turn_state_cache (
    account_id TEXT NOT NULL REFERENCES turn_state_settings(account_id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    owner TEXT NOT NULL,
    revision TEXT NOT NULL,
    token TEXT,
    issued_at INTEGER NOT NULL DEFAULT 0,
    expires_at INTEGER NOT NULL DEFAULT 0,
    refresh_at INTEGER NOT NULL DEFAULT 0,
    next_probe_at INTEGER NOT NULL DEFAULT 0,
    lease_until INTEGER NOT NULL DEFAULT 0,
    lease TEXT NOT NULL DEFAULT '',
    last_probe_at INTEGER NOT NULL DEFAULT 0,
    probe_status INTEGER NOT NULL DEFAULT 0,
    probe_result TEXT NOT NULL DEFAULT 'waiting',
    injections INTEGER NOT NULL DEFAULT 0,
    client_states INTEGER NOT NULL DEFAULT 0,
    strikes INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(account_id, model)
);
-- Reauthorization and token refresh invalidate opaque routing state conservatively.
CREATE TRIGGER turn_state_auth_change AFTER UPDATE ON account_tokens BEGIN
    UPDATE turn_state_settings SET revision=lower(hex(randomblob(16))) WHERE account_id=NEW.account_id;
    DELETE FROM turn_state_cache WHERE account_id = NEW.account_id;
END;
CREATE TRIGGER turn_state_identity_change AFTER UPDATE OF chatgpt_account_id, proxy_id, http_fingerprint_json ON accounts
WHEN OLD.chatgpt_account_id IS NOT NEW.chatgpt_account_id OR OLD.proxy_id IS NOT NEW.proxy_id OR OLD.http_fingerprint_json IS NOT NEW.http_fingerprint_json BEGIN
    UPDATE turn_state_settings SET revision=lower(hex(randomblob(16))) WHERE account_id=NEW.id;
    DELETE FROM turn_state_cache WHERE account_id = NEW.id;
END;

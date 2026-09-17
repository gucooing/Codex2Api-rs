DELETE FROM proxy_api_keys WHERE revoked_at IS NOT NULL;
ALTER TABLE proxy_api_keys DROP COLUMN revoked_at;
ALTER TABLE proxy_api_keys ADD COLUMN paused_at TEXT;
ALTER TABLE proxy_api_keys ADD COLUMN key_token TEXT;

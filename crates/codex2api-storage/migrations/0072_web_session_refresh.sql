-- Browser logins now have separate access and refresh JWTs. Existing single-token
-- logins must authenticate once; account identities and Codex OAuth are unchanged.
DELETE FROM oauth_browser_identities WHERE session_hash IS NOT NULL;
DELETE FROM user_sessions;
DELETE FROM admin_sessions;
ALTER TABLE user_sessions RENAME COLUMN token_hash TO id;
ALTER TABLE oauth_browser_identities RENAME COLUMN session_hash TO session_id;
ALTER TABLE user_sessions ADD COLUMN refresh_version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE user_sessions ADD COLUMN refresh_issued_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE admin_sessions ADD COLUMN refresh_version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE admin_sessions ADD COLUMN refresh_issued_at INTEGER NOT NULL DEFAULT 0;

CREATE TABLE oauth_browser_flows (
    id TEXT PRIMARY KEY NOT NULL,
    cookie_hash TEXT NOT NULL,
    csrf_hash TEXT NOT NULL,
    request_json TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX oauth_browser_expiry ON oauth_browser_flows(expires_at);

CREATE TABLE oauth_authorization_codes (
    code_hash TEXT PRIMARY KEY NOT NULL,
    credential_id TEXT NOT NULL REFERENCES oauth_credentials(id) ON DELETE CASCADE,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX oauth_code_expiry ON oauth_authorization_codes(expires_at);

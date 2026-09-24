-- Preserve existing grants; new authorizations persist the scopes actually requested.
ALTER TABLE virtual_devices ADD COLUMN scopes TEXT NOT NULL DEFAULT 'openid profile email offline_access api.connectors.read api.connectors.invoke';
ALTER TABLE virtual_authorization_codes ADD COLUMN scopes TEXT NOT NULL DEFAULT 'openid profile email offline_access api.connectors.read api.connectors.invoke';
ALTER TABLE virtual_access_tokens ADD COLUMN scopes TEXT NOT NULL DEFAULT 'openid profile email offline_access api.connectors.read api.connectors.invoke';

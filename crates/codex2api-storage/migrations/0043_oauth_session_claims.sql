-- Capture the actual local authentication and authorization-request timestamps.
-- Refresh must not claim that the password was entered again.
ALTER TABLE virtual_authorization_codes ADD COLUMN authenticated_at_ms INTEGER;
ALTER TABLE virtual_authorization_codes ADD COLUMN requested_at_ms INTEGER;
ALTER TABLE virtual_devices ADD COLUMN authenticated_at_ms INTEGER;
ALTER TABLE virtual_devices ADD COLUMN requested_at_ms INTEGER;
UPDATE virtual_devices
SET authenticated_at_ms=unixepoch(created_at)*1000, requested_at_ms=unixepoch(created_at)*1000;

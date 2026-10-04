-- Preserve the original device authorization's requested scopes through consent.
ALTER TABLE virtual_device_authorizations ADD COLUMN requested_scopes TEXT;

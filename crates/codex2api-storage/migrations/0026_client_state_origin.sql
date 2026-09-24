ALTER TABLE virtual_client_state ADD COLUMN write_origin TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE virtual_client_state ADD COLUMN updated_at_ms INTEGER NOT NULL DEFAULT 0;

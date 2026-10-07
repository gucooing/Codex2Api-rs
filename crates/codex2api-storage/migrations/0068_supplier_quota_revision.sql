-- Order quota observations independently of authorization recovery.
ALTER TABLE supplier_health ADD COLUMN cooldown_revision INTEGER NOT NULL DEFAULT 0;

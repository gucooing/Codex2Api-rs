-- An unbound route retains its revision, preventing stale administrator writes.
DROP TRIGGER execution_route_provider_insert;
DROP TRIGGER execution_route_provider_update;
CREATE TABLE execution_routes_v2 (
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    supplier_account_id TEXT REFERENCES supplier_accounts(id) ON DELETE SET NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY(virtual_account_id,provider_id),
    UNIQUE(virtual_account_id)
);
INSERT INTO execution_routes_v2 SELECT * FROM execution_routes;
DROP TABLE execution_routes;
ALTER TABLE execution_routes_v2 RENAME TO execution_routes;
CREATE TRIGGER execution_route_provider_insert BEFORE INSERT ON execution_routes
WHEN NOT EXISTS(SELECT 1 FROM virtual_accounts v WHERE v.id=NEW.virtual_account_id AND v.provider_id=NEW.provider_id)
 OR (NEW.supplier_account_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_accounts s WHERE s.id=NEW.supplier_account_id AND s.provider_id=NEW.provider_id))
BEGIN SELECT RAISE(ABORT,'execution route provider mismatch'); END;
CREATE TRIGGER execution_route_provider_update BEFORE UPDATE ON execution_routes
WHEN NEW.virtual_account_id!=OLD.virtual_account_id OR NEW.provider_id!=OLD.provider_id
 OR (NEW.supplier_account_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_accounts s WHERE s.id=NEW.supplier_account_id AND s.provider_id=NEW.provider_id))
BEGIN SELECT RAISE(ABORT,'execution route provider mismatch'); END;
CREATE TRIGGER execution_route_supplier_removed AFTER UPDATE OF supplier_account_id ON execution_routes
WHEN OLD.supplier_account_id IS NOT NULL AND NEW.supplier_account_id IS NULL AND NEW.revision=OLD.revision
BEGIN UPDATE execution_routes SET revision=revision+1 WHERE virtual_account_id=NEW.virtual_account_id; END;

-- Routes select pools only; individual suppliers are chosen per generation.
DROP VIEW effective_execution_routes;
DROP TRIGGER execution_route_provider_insert;
DROP TRIGGER execution_route_provider_update;
DROP TRIGGER execution_route_supplier_removed;
DROP TRIGGER execution_route_pool_insert;
DROP TRIGGER execution_route_pool_update;
DROP TRIGGER supplier_tag_member_removed;
DROP TRIGGER plan_supplier_pool_changed;
DROP TRIGGER subscription_supplier_pool_changed;
CREATE TABLE execution_routes_new (
    virtual_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    tag_id TEXT REFERENCES supplier_tags(id) ON DELETE RESTRICT,
    revision INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY(virtual_account_id,provider_id), UNIQUE(virtual_account_id)
);
INSERT INTO execution_routes_new SELECT virtual_account_id,provider_id,tag_id,revision+1 FROM execution_routes;
DROP TABLE execution_routes;
ALTER TABLE execution_routes_new RENAME TO execution_routes;
CREATE INDEX execution_route_tag ON execution_routes(tag_id);
CREATE VIEW effective_execution_routes AS
SELECT r.virtual_account_id,r.provider_id,r.revision,
       CASE WHEN p.virtual_account_id IS NOT NULL THEN p.tag_id ELSE r.tag_id END AS tag_id
FROM execution_routes r LEFT JOIN regular_subscription_pools p ON p.virtual_account_id=r.virtual_account_id;
CREATE TRIGGER execution_route_provider_insert BEFORE INSERT ON execution_routes
WHEN NOT EXISTS(SELECT 1 FROM platform_accounts p WHERE p.id=NEW.virtual_account_id AND p.provider_id=NEW.provider_id)
 OR (NEW.tag_id IS NOT NULL AND (EXISTS(SELECT 1 FROM regular_platforms WHERE id=NEW.virtual_account_id)
 OR NOT EXISTS(SELECT 1 FROM supplier_tags WHERE id=NEW.tag_id AND provider_id=NEW.provider_id)))
BEGIN SELECT RAISE(ABORT,'execution route pool mismatch'); END;
CREATE TRIGGER execution_route_provider_update BEFORE UPDATE ON execution_routes
WHEN NEW.virtual_account_id!=OLD.virtual_account_id OR NEW.provider_id!=OLD.provider_id
 OR (NEW.tag_id IS NOT NULL AND (EXISTS(SELECT 1 FROM regular_platforms WHERE id=NEW.virtual_account_id)
 OR NOT EXISTS(SELECT 1 FROM supplier_tags WHERE id=NEW.tag_id AND provider_id=NEW.provider_id)))
BEGIN SELECT RAISE(ABORT,'execution route pool mismatch'); END;

CREATE TABLE virtual_concurrency_limits (
    virtual_account_id TEXT PRIMARY KEY REFERENCES platform_accounts(id) ON DELETE CASCADE,
    concurrency INTEGER NOT NULL CHECK(concurrency BETWEEN 0 AND 1000000)
);
DROP TABLE virtual_request_admissions;
CREATE TABLE supplier_scheduling (
    account_id TEXT PRIMARY KEY REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    concurrency INTEGER NOT NULL DEFAULT 5 CHECK(concurrency BETWEEN 0 AND 1000000),
    priority INTEGER NOT NULL DEFAULT 0 CHECK(priority BETWEEN 0 AND 1000000),
    load_factor INTEGER CHECK(load_factor BETWEEN 1 AND 1000000),
    revision INTEGER NOT NULL DEFAULT 1
);
INSERT INTO supplier_scheduling(account_id) SELECT id FROM supplier_accounts;
CREATE TABLE supplier_models (
    account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    source TEXT NOT NULL CHECK(source IN ('upstream','custom')),
    descriptor_json TEXT NOT NULL DEFAULT '{}',
    observed_at TEXT,
    PRIMARY KEY(account_id,model)
);
CREATE INDEX supplier_models_model ON supplier_models(model,account_id);
CREATE TABLE supplier_model_sync (
    account_id TEXT PRIMARY KEY REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    observed_at TEXT NOT NULL,
    auth_revision INTEGER NOT NULL
);
INSERT INTO supplier_models SELECT account_id,model,'upstream',descriptor_json,observed_at FROM grok_model_observations;
INSERT INTO supplier_model_sync SELECT account_id,observed_at,auth_revision FROM grok_catalog_snapshots;
DROP TABLE grok_model_observations;
DROP TABLE grok_catalog_snapshots;

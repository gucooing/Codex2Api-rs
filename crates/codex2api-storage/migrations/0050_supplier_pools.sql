CREATE TABLE supplier_tags (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    name TEXT NOT NULL COLLATE NOCASE CHECK(length(trim(name)) BETWEEN 1 AND 80),
    UNIQUE(provider_id,name)
);
CREATE TABLE supplier_tag_members (
    tag_id TEXT NOT NULL REFERENCES supplier_tags(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    PRIMARY KEY(tag_id,account_id)
);
CREATE INDEX supplier_tag_account ON supplier_tag_members(account_id);
CREATE TRIGGER supplier_tag_provider_insert BEFORE INSERT ON supplier_tag_members
WHEN NOT EXISTS(SELECT 1 FROM supplier_tags t JOIN supplier_accounts a ON a.id=NEW.account_id WHERE t.id=NEW.tag_id AND t.provider_id=a.provider_id)
BEGIN SELECT RAISE(ABORT,'supplier tag provider mismatch'); END;
CREATE TRIGGER supplier_tag_provider_immutable BEFORE UPDATE OF provider_id ON supplier_tags
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'supplier tag provider is immutable'); END;

ALTER TABLE execution_routes ADD COLUMN tag_id TEXT REFERENCES supplier_tags(id) ON DELETE RESTRICT;
-- Preserve existing isolation and bindings. Administrators can subsequently merge pools.
INSERT INTO supplier_tags(id,provider_id,name)
SELECT 'legacy-'||s.id,s.provider_id,'迁移号池 '||s.id FROM supplier_accounts s
WHERE EXISTS(SELECT 1 FROM execution_routes r WHERE r.supplier_account_id=s.id);
INSERT INTO supplier_tag_members(tag_id,account_id)
SELECT id,substr(id,8) FROM supplier_tags;
UPDATE execution_routes SET tag_id='legacy-'||supplier_account_id WHERE supplier_account_id IS NOT NULL;
CREATE INDEX execution_route_supplier ON execution_routes(supplier_account_id);
CREATE INDEX execution_route_tag ON execution_routes(tag_id);
CREATE TRIGGER execution_route_pool_insert BEFORE INSERT ON execution_routes
WHEN (NEW.tag_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_tags t WHERE t.id=NEW.tag_id AND t.provider_id=NEW.provider_id))
 OR (NEW.supplier_account_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_tag_members m WHERE m.tag_id=NEW.tag_id AND m.account_id=NEW.supplier_account_id))
BEGIN SELECT RAISE(ABORT,'execution route pool mismatch'); END;
CREATE TRIGGER execution_route_pool_update BEFORE UPDATE OF tag_id,supplier_account_id ON execution_routes
WHEN (NEW.tag_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_tags t WHERE t.id=NEW.tag_id AND t.provider_id=NEW.provider_id))
 OR (NEW.supplier_account_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_tag_members m WHERE m.tag_id=NEW.tag_id AND m.account_id=NEW.supplier_account_id))
BEGIN SELECT RAISE(ABORT,'execution route pool mismatch'); END;
CREATE TRIGGER supplier_tag_member_removed AFTER DELETE ON supplier_tag_members
BEGIN UPDATE execution_routes SET supplier_account_id=NULL,revision=revision+1
WHERE tag_id=OLD.tag_id AND supplier_account_id=OLD.account_id; END;

ALTER TABLE supplier_health ADD COLUMN cooldown_kind TEXT CHECK(cooldown_kind IN ('rate_limited','quota_exhausted'));
ALTER TABLE supplier_health ADD COLUMN cooldown_until INTEGER;
ALTER TABLE supplier_health ADD COLUMN cooldown_auth_revision INTEGER;
ALTER TABLE supplier_health ADD COLUMN cooldown_code TEXT;
ALTER TABLE supplier_health ADD COLUMN cooldown_observed_at INTEGER;

CREATE TABLE virtual_rpm_limits (
    virtual_account_id TEXT PRIMARY KEY REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    rpm INTEGER NOT NULL CHECK(rpm BETWEEN 0 AND 1000000)
);
CREATE TABLE virtual_request_admissions (
    id INTEGER PRIMARY KEY,
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    admitted_at_ms INTEGER NOT NULL
);
CREATE INDEX virtual_admissions_owner_time ON virtual_request_admissions(virtual_account_id,admitted_at_ms);
CREATE INDEX virtual_admissions_time ON virtual_request_admissions(admitted_at_ms);

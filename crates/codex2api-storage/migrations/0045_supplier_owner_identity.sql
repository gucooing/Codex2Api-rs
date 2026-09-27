-- A ChatGPT account ID identifies a workspace, shared by its Team members.
-- Storage::migrate uses a dedicated connection with foreign keys disabled so
-- rebuilding the parent cannot cascade-delete credentials, routes or snapshots.
CREATE TEMP TABLE supplier_identity_migration_check (valid INTEGER CHECK(valid = 1));
INSERT INTO supplier_identity_migration_check SELECT foreign_keys = 0 FROM pragma_foreign_keys;

DROP TRIGGER supplier_auth_insert;
DROP TRIGGER supplier_auth_update;
DROP TRIGGER supplier_auth_delete;
DROP TRIGGER execution_route_provider_insert;
DROP TRIGGER execution_route_provider_update;

CREATE TABLE supplier_accounts_next (
    id TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL,
    display_name TEXT,
    chatgpt_account_id TEXT,
    chatgpt_user_id TEXT,
    email TEXT,
    plan_type TEXT,
    installation_id TEXT NOT NULL,
    originator TEXT NOT NULL,
    user_agent TEXT NOT NULL,
    os_type TEXT NOT NULL,
    os_version TEXT NOT NULL,
    arch TEXT NOT NULL,
    home_dir TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_used_at TEXT,
    http_fingerprint_json TEXT NOT NULL DEFAULT '{}',
    proxy_id TEXT REFERENCES outbound_proxies(id) ON DELETE RESTRICT,
    provider_id TEXT NOT NULL DEFAULT 'chatgpt',
    auth_revision INTEGER NOT NULL DEFAULT 0,
    UNIQUE (provider_id, chatgpt_account_id, chatgpt_user_id)
);
INSERT INTO supplier_accounts_next SELECT * FROM supplier_accounts;
DROP TABLE supplier_accounts;
ALTER TABLE supplier_accounts_next RENAME TO supplier_accounts;

CREATE TRIGGER supplier_provider_registered BEFORE INSERT ON supplier_accounts
WHEN NOT EXISTS(SELECT 1 FROM providers p WHERE p.id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'unknown supplier provider'); END;
CREATE TRIGGER supplier_provider_immutable BEFORE UPDATE OF provider_id ON supplier_accounts
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'supplier provider is immutable'); END;
CREATE TRIGGER supplier_workspace_identity_update AFTER UPDATE OF chatgpt_account_id,chatgpt_user_id ON supplier_accounts
WHEN NEW.chatgpt_account_id IS NOT OLD.chatgpt_account_id OR NEW.chatgpt_user_id IS NOT OLD.chatgpt_user_id
BEGIN UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=NEW.id; END;

CREATE TRIGGER supplier_auth_insert AFTER INSERT ON supplier_tokens
BEGIN UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=NEW.account_id; END;
CREATE TRIGGER supplier_auth_update AFTER UPDATE ON supplier_tokens
WHEN NEW.access_token IS NOT OLD.access_token OR NEW.id_token IS NOT OLD.id_token
  OR NEW.refresh_token IS NOT OLD.refresh_token OR NEW.raw_auth_json IS NOT OLD.raw_auth_json
BEGIN UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=NEW.account_id; END;
CREATE TRIGGER supplier_auth_delete AFTER DELETE ON supplier_tokens
BEGIN UPDATE supplier_accounts SET auth_revision=auth_revision+1 WHERE id=OLD.account_id; END;

CREATE TRIGGER execution_route_provider_insert BEFORE INSERT ON execution_routes
WHEN NOT EXISTS(SELECT 1 FROM virtual_accounts v WHERE v.id=NEW.virtual_account_id AND v.provider_id=NEW.provider_id)
 OR (NEW.supplier_account_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_accounts s WHERE s.id=NEW.supplier_account_id AND s.provider_id=NEW.provider_id))
BEGIN SELECT RAISE(ABORT,'execution route provider mismatch'); END;
CREATE TRIGGER execution_route_provider_update BEFORE UPDATE ON execution_routes
WHEN NEW.virtual_account_id!=OLD.virtual_account_id OR NEW.provider_id!=OLD.provider_id
 OR (NEW.supplier_account_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM supplier_accounts s WHERE s.id=NEW.supplier_account_id AND s.provider_id=NEW.provider_id))
BEGIN SELECT RAISE(ABORT,'execution route provider mismatch'); END;

-- Fail inside the migration transaction, before its schema/data changes commit.
INSERT INTO supplier_identity_migration_check SELECT 0 FROM pragma_foreign_key_check;
DROP TABLE supplier_identity_migration_check;

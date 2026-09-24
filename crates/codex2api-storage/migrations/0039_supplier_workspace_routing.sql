-- Bind official routing snapshots to the exact supplier credential generation.
ALTER TABLE supplier_accounts ADD COLUMN auth_revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE supplier_info_cache ADD COLUMN auth_revision INTEGER;

CREATE TRIGGER supplier_auth_insert AFTER INSERT ON supplier_tokens
BEGIN
    UPDATE supplier_accounts SET auth_revision = auth_revision + 1 WHERE id = NEW.account_id;
END;
CREATE TRIGGER supplier_auth_update AFTER UPDATE ON supplier_tokens
WHEN NEW.access_token IS NOT OLD.access_token OR NEW.id_token IS NOT OLD.id_token
  OR NEW.refresh_token IS NOT OLD.refresh_token OR NEW.raw_auth_json IS NOT OLD.raw_auth_json
BEGIN
    UPDATE supplier_accounts SET auth_revision = auth_revision + 1 WHERE id = NEW.account_id;
END;
CREATE TRIGGER supplier_auth_delete AFTER DELETE ON supplier_tokens
BEGIN
    UPDATE supplier_accounts SET auth_revision = auth_revision + 1 WHERE id = OLD.account_id;
END;
CREATE TRIGGER supplier_workspace_identity_update AFTER UPDATE OF chatgpt_account_id ON supplier_accounts
WHEN NEW.chatgpt_account_id IS NOT OLD.chatgpt_account_id
BEGIN
    UPDATE supplier_accounts SET auth_revision = auth_revision + 1 WHERE id = NEW.id;
END;

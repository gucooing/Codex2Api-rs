-- Consumers own subscriptions and history. Suppliers only execute routed requests.
DROP TRIGGER virtual_account_provider_insert;
DROP TRIGGER virtual_account_provider_update;
DROP TRIGGER plan_provider_immutable;

ALTER TABLE accounts RENAME TO supplier_accounts;
ALTER TABLE account_tokens RENAME TO supplier_tokens;
ALTER TABLE account_runtime RENAME TO supplier_runtime;

CREATE TABLE execution_routes (
    virtual_account_id TEXT NOT NULL REFERENCES virtual_accounts(id) ON DELETE CASCADE,
    provider_id TEXT NOT NULL REFERENCES providers(id),
    supplier_account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY(virtual_account_id,provider_id)
);
INSERT INTO execution_routes(virtual_account_id,provider_id,supplier_account_id)
SELECT id,provider_id,account_id FROM virtual_accounts WHERE account_id IS NOT NULL;
CREATE TRIGGER execution_route_provider_insert BEFORE INSERT ON execution_routes
WHEN NOT EXISTS(SELECT 1 FROM supplier_accounts s JOIN virtual_accounts v ON v.id=NEW.virtual_account_id WHERE s.id=NEW.supplier_account_id AND s.provider_id=NEW.provider_id AND v.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'execution route provider mismatch'); END;
CREATE TRIGGER execution_route_provider_update BEFORE UPDATE ON execution_routes
WHEN NEW.virtual_account_id!=OLD.virtual_account_id OR NEW.provider_id!=OLD.provider_id
 OR NOT EXISTS(SELECT 1 FROM supplier_accounts s JOIN virtual_accounts v ON v.id=NEW.virtual_account_id WHERE s.id=NEW.supplier_account_id AND s.provider_id=NEW.provider_id AND v.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'execution route provider mismatch'); END;

DROP INDEX virtual_accounts_binding;
ALTER TABLE virtual_accounts DROP COLUMN account_id;

-- Model names and consumer plans remain scoped to exactly one provider.
UPDATE virtual_plans SET config=json_set(config,'$.models',json(COALESCE((
    SELECT json_group_array(json_object('provider_id','chatgpt','model',value))
    FROM json_each(virtual_plans.config,'$.models')
),'[]')));

-- OAuth sessions are scoped to their protocol provider, independent of suppliers.
ALTER TABLE virtual_devices ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'chatgpt';
ALTER TABLE virtual_authorization_codes ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'chatgpt';

CREATE TRIGGER supplier_provider_registered BEFORE INSERT ON supplier_accounts
WHEN NOT EXISTS(SELECT 1 FROM providers p WHERE p.id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'unknown supplier provider'); END;
CREATE TRIGGER device_provider_registered BEFORE INSERT ON virtual_devices
WHEN NOT EXISTS(SELECT 1 FROM providers p WHERE p.id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'unknown session provider'); END;
CREATE TRIGGER device_provider_immutable BEFORE UPDATE OF provider_id ON virtual_devices
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'session provider is immutable'); END;
CREATE TRIGGER authorization_provider_registered BEFORE INSERT ON virtual_authorization_codes
WHEN NOT EXISTS(SELECT 1 FROM providers p WHERE p.id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'unknown authorization provider'); END;

CREATE UNIQUE INDEX consumer_execution_route ON execution_routes(virtual_account_id);
CREATE TRIGGER consumer_plan_provider_insert BEFORE INSERT ON virtual_accounts
WHEN NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'consumer plan provider mismatch'); END;
CREATE TRIGGER consumer_plan_provider_update BEFORE UPDATE OF plan_id,provider_id ON virtual_accounts
WHEN NEW.provider_id!=OLD.provider_id OR NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'consumer provider is immutable; plan must match'); END;
CREATE TRIGGER consumer_session_provider BEFORE INSERT ON virtual_devices
WHEN NOT EXISTS(SELECT 1 FROM virtual_accounts v WHERE v.id=NEW.virtual_account_id AND v.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'consumer session provider mismatch'); END;
CREATE TRIGGER consumer_authorization_provider BEFORE INSERT ON virtual_authorization_codes
WHEN NOT EXISTS(SELECT 1 FROM virtual_accounts v WHERE v.id=NEW.virtual_account_id AND v.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'consumer authorization provider mismatch'); END;
CREATE TRIGGER plan_provider_immutable BEFORE UPDATE OF provider_id ON virtual_plans
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'plan provider is immutable'); END;

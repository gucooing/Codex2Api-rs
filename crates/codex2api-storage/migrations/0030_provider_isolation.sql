CREATE TABLE providers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL
);
INSERT INTO providers(id,name) VALUES('chatgpt','ChatGPT');

ALTER TABLE accounts ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'chatgpt';
ALTER TABLE virtual_accounts ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'chatgpt';
ALTER TABLE virtual_plans ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'chatgpt';
ALTER TABLE usage_records ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'chatgpt';

-- Binding and subscription identity must agree even if a writer bypasses the UI.
CREATE TRIGGER virtual_account_provider_insert BEFORE INSERT ON virtual_accounts
WHEN (NEW.account_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM accounts a WHERE a.id=NEW.account_id AND a.provider_id=NEW.provider_id))
  OR NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'virtual account provider mismatch'); END;
CREATE TRIGGER virtual_account_provider_update BEFORE UPDATE OF account_id,plan_id,provider_id ON virtual_accounts
WHEN NEW.provider_id!=OLD.provider_id
  OR (NEW.account_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM accounts a WHERE a.id=NEW.account_id AND a.provider_id=NEW.provider_id))
  OR NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'virtual account provider mismatch'); END;
CREATE TRIGGER supplier_provider_immutable BEFORE UPDATE OF provider_id ON accounts
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'supplier provider is immutable'); END;
CREATE TRIGGER plan_provider_immutable BEFORE UPDATE OF provider_id ON virtual_plans
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'plan provider is immutable'); END;

CREATE TABLE model_catalog_next (
    provider_id TEXT NOT NULL REFERENCES providers(id),
    model TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('text','image')),
    enabled INTEGER NOT NULL DEFAULT 1,
    deleted INTEGER NOT NULL DEFAULT 0,
    revision INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY(provider_id,model)
);
INSERT INTO model_catalog_next SELECT 'chatgpt',model,kind,enabled,deleted,revision FROM model_catalog;
CREATE TABLE model_image_prices_next (
    provider_id TEXT NOT NULL,
    model TEXT NOT NULL,
    resolution TEXT NOT NULL,
    price_nano_usd INTEGER NOT NULL CHECK(price_nano_usd>=0),
    PRIMARY KEY(provider_id,model,resolution),
    FOREIGN KEY(provider_id,model) REFERENCES model_catalog_next(provider_id,model)
);
INSERT INTO model_image_prices_next SELECT 'chatgpt',model,resolution,price_nano_usd FROM model_image_prices;
DROP TABLE model_image_prices;
DROP TABLE model_catalog;
ALTER TABLE model_catalog_next RENAME TO model_catalog;
ALTER TABLE model_image_prices_next RENAME TO model_image_prices;

CREATE TABLE model_prices_next (
    provider_id TEXT NOT NULL REFERENCES providers(id),
    model TEXT NOT NULL,
    tier TEXT NOT NULL,
    min_input_tokens INTEGER NOT NULL DEFAULT 0,
    input_rate INTEGER NOT NULL CHECK(input_rate>=0),
    cached_rate INTEGER NOT NULL CHECK(cached_rate>=0),
    cache_write_rate INTEGER NOT NULL CHECK(cache_write_rate>=0),
    output_rate INTEGER NOT NULL CHECK(output_rate>=0),
    source TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY(provider_id,model,tier,min_input_tokens)
);
INSERT INTO model_prices_next SELECT 'chatgpt',model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source,revision FROM model_prices;
DROP TABLE model_prices;
ALTER TABLE model_prices_next RENAME TO model_prices;

-- The empty-list convention must never represent both no benefits and all models.
UPDATE virtual_plans SET config=json_set(config,
    '$.model_access',CASE WHEN json_array_length(config,'$.models')=0 THEN 'all' ELSE 'selected' END,
    '$.free_model_access','none','$.free_models',json('[]'));

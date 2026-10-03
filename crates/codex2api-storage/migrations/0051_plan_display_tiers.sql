DROP TRIGGER consumer_plan_provider_insert;
DROP TRIGGER consumer_plan_provider_update;
DROP TRIGGER plan_provider_immutable;
CREATE TABLE virtual_plans_v2 (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    plan_type TEXT NOT NULL CHECK(plan_type IN ('free','go','plus','prolite','pro','promax','team','business','enterprise','edu','edu_plus','edu_pro','self_serve_business_prolite','self_serve_business_usage_based','ent26','enterprise_cbp_automation','enterprise_cbp_usage_based')),
    config TEXT NOT NULL CHECK(json_valid(config)),
    enabled INTEGER NOT NULL DEFAULT 1,
    revision INTEGER NOT NULL DEFAULT 0,
    updated_at_ms INTEGER NOT NULL DEFAULT 0,
    provider_id TEXT NOT NULL DEFAULT 'chatgpt' REFERENCES providers(id)
);
INSERT INTO virtual_plans_v2 SELECT id,name,plan_type,config,enabled,revision,updated_at_ms,provider_id FROM virtual_plans;
DROP TABLE virtual_plans;
ALTER TABLE virtual_plans_v2 RENAME TO virtual_plans;
UPDATE virtual_plans SET name=CASE plan_type
    WHEN 'free' THEN 'ChatGPT Free' WHEN 'plus' THEN 'ChatGPT Plus'
    WHEN 'pro' THEN 'ChatGPT Pro 200' WHEN 'business' THEN 'ChatGPT Business'
    WHEN 'enterprise' THEN 'ChatGPT Enterprise' WHEN 'edu' THEN 'ChatGPT Edu' END
WHERE id=plan_type AND name=plan_type;
CREATE TRIGGER consumer_plan_provider_insert BEFORE INSERT ON virtual_accounts
WHEN NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'consumer plan provider mismatch'); END;
CREATE TRIGGER consumer_plan_provider_update BEFORE UPDATE OF plan_id,provider_id ON virtual_accounts
WHEN NEW.provider_id!=OLD.provider_id OR NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'consumer provider is immutable; plan must match'); END;
CREATE TRIGGER plan_provider_immutable BEFORE UPDATE OF provider_id ON virtual_plans
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'plan provider is immutable'); END;

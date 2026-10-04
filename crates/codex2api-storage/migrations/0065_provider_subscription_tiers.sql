-- Subscription tiers belong to their provider. Preserve all plan IDs, prices and history.
DROP TRIGGER consumer_plan_provider_insert;
DROP TRIGGER consumer_plan_provider_update;
DROP TRIGGER plan_provider_immutable;
DROP TRIGGER preserve_free_plan_delete;
DROP TRIGGER preserve_free_plan_type;
DROP TRIGGER platform_free_plan_valid;
CREATE TABLE virtual_plans_provider_tiers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    plan_type TEXT NOT NULL,
    config TEXT NOT NULL CHECK(json_valid(config)),
    allow_purchase INTEGER NOT NULL DEFAULT 1,
    revision INTEGER NOT NULL DEFAULT 0,
    updated_at_ms INTEGER NOT NULL DEFAULT 0,
    provider_id TEXT NOT NULL DEFAULT 'chatgpt' REFERENCES providers(id),
    CHECK((provider_id='chatgpt' AND plan_type IN ('free','go','plus','prolite','pro','promax','team','business','enterprise','edu','edu_plus','edu_pro','self_serve_business_prolite','self_serve_business_usage_based','ent26','enterprise_cbp_automation','enterprise_cbp_usage_based'))
       OR (provider_id='grok' AND plan_type IN ('free','premium','premium_plus','supergrok','supergrok_heavy','supergrok_pro','team')))
);
INSERT INTO virtual_plans_provider_tiers SELECT id,name,plan_type,config,allow_purchase,revision,updated_at_ms,provider_id FROM virtual_plans;
DROP TABLE virtual_plans;
ALTER TABLE virtual_plans_provider_tiers RENAME TO virtual_plans;
CREATE TRIGGER consumer_plan_provider_insert BEFORE INSERT ON virtual_accounts
WHEN NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'consumer plan provider mismatch'); END;
CREATE TRIGGER consumer_plan_provider_update BEFORE UPDATE OF plan_id,provider_id ON virtual_accounts
WHEN NEW.provider_id!=OLD.provider_id OR NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.id=NEW.plan_id AND p.provider_id=NEW.provider_id)
BEGIN SELECT RAISE(ABORT,'consumer provider is immutable; plan must match'); END;
CREATE TRIGGER plan_provider_immutable BEFORE UPDATE OF provider_id ON virtual_plans
WHEN NEW.provider_id!=OLD.provider_id
BEGIN SELECT RAISE(ABORT,'plan provider is immutable'); END;
CREATE TRIGGER preserve_free_plan_delete BEFORE DELETE ON virtual_plans WHEN OLD.plan_type='free'
BEGIN SELECT RAISE(ABORT,'Free plans cannot be deleted'); END;
CREATE TRIGGER preserve_free_plan_type BEFORE UPDATE OF plan_type ON virtual_plans
WHEN OLD.plan_type='free' AND NEW.plan_type!='free'
BEGIN SELECT RAISE(ABORT,'Free plan tier is immutable'); END;
CREATE TRIGGER platform_free_plan_valid BEFORE INSERT ON platform_free_plans
WHEN NOT EXISTS(SELECT 1 FROM virtual_plans WHERE id=NEW.plan_id AND provider_id=NEW.provider_id AND plan_type='free')
BEGIN SELECT RAISE(ABORT,'invalid platform Free plan'); END;

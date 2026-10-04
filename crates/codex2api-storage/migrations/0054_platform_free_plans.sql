-- Each platform has an explicit, administrable Free policy. Paid-plan fallback
-- controls are removed; historical purchases and independent accounts are kept.
INSERT INTO virtual_plans(id,provider_id,name,plan_type,config,enabled)
SELECT 'platform-free-chatgpt','chatgpt','ChatGPT Free','free',
    '{"model_access":"none","models":[],"spending_windows":[{"duration_seconds":2592000,"cost_limit_usd":"0"}],"duration_days":30}',1
WHERE NOT EXISTS(SELECT 1 FROM virtual_plans WHERE provider_id='chatgpt' AND plan_type='free');
CREATE TABLE platform_free_plans (
    provider_id TEXT PRIMARY KEY REFERENCES providers(id),
    plan_id TEXT NOT NULL UNIQUE REFERENCES virtual_plans(id) ON DELETE RESTRICT
);
INSERT INTO platform_free_plans(provider_id,plan_id)
SELECT 'chatgpt',id FROM virtual_plans WHERE provider_id='chatgpt' AND plan_type='free'
ORDER BY CASE WHEN id='free' THEN 0 ELSE 1 END,id LIMIT 1;
UPDATE virtual_plans SET config=json_set(config,'$.model_access','none','$.models',json('[]'))
WHERE plan_type='free' AND COALESCE(json_extract(config,'$.free_access_enabled'),0)=0;
UPDATE virtual_plans SET config=json_remove(config,'$.free_access_enabled','$.free_model_access','$.free_models',
    '$.free_spending_windows','$.free_primary_cost_limit_usd','$.free_weekly_cost_limit_usd');
CREATE TRIGGER preserve_free_plan_delete BEFORE DELETE ON virtual_plans WHEN OLD.plan_type='free'
BEGIN SELECT RAISE(ABORT,'Free plans cannot be deleted'); END;
CREATE TRIGGER preserve_free_plan_type BEFORE UPDATE OF plan_type ON virtual_plans
WHEN OLD.plan_type='free' AND NEW.plan_type!='free'
BEGIN SELECT RAISE(ABORT,'Free plan tier is immutable'); END;
CREATE TRIGGER platform_free_plan_valid BEFORE INSERT ON platform_free_plans
WHEN NOT EXISTS(SELECT 1 FROM virtual_plans WHERE id=NEW.plan_id AND provider_id=NEW.provider_id AND plan_type='free')
BEGIN SELECT RAISE(ABORT,'invalid platform Free plan'); END;

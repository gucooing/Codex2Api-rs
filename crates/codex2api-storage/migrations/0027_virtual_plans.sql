CREATE TABLE virtual_plans (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    plan_type TEXT NOT NULL CHECK(plan_type IN ('free','plus','pro','business','enterprise','edu')),
    config TEXT NOT NULL CHECK(json_valid(config)),
    enabled INTEGER NOT NULL DEFAULT 1,
    revision INTEGER NOT NULL DEFAULT 0,
    updated_at_ms INTEGER NOT NULL DEFAULT 0
);

INSERT INTO virtual_plans(id,name,plan_type,config)
SELECT value, value, value, json_object(
    'models',json('[]'),
    'primary_cost_limit_usd',CASE WHEN value='free' THEN 0 ELSE NULL END,
    'weekly_cost_limit_usd',CASE WHEN value='free' THEN 0 ELSE NULL END,
    'free_access_enabled',json('false'),
    'free_primary_cost_limit_usd',0,'free_weekly_cost_limit_usd',0
) FROM json_each('["free","plus","pro","business","enterprise","edu"]');

ALTER TABLE virtual_accounts ADD COLUMN plan_id TEXT REFERENCES virtual_plans(id) ON DELETE RESTRICT;

-- Preserve each distinct effective legacy configuration as a reusable plan.
-- Keep the old configuration rows as migration history; runtime reads use the catalog.
CREATE TEMP TABLE virtual_plan_migration AS
WITH old AS (
    SELECT a.id,a.username,a.plan_type,
        COALESCE(q.value_json,'{}') AS quota,
        COALESCE(json_extract(e.value_json,'$.'||a.plan_type),'{}') AS entitlement,
        COALESCE(p.value_json,'{"free_access_enabled":false,"primary_cost_limit_usd":0,"weekly_cost_limit_usd":0}') AS policy
    FROM virtual_accounts a
    LEFT JOIN virtual_client_state q ON q.virtual_account_id=a.id AND q.state_key='quota'
    LEFT JOIN virtual_client_state e ON e.virtual_account_id=a.id AND e.state_key='subscription_entitlements'
    LEFT JOIN virtual_client_state p ON p.virtual_account_id=a.id AND p.state_key='subscription_policy'
)
SELECT id,username,plan_type,json_object(
    'models',json(COALESCE(json_extract(entitlement,'$.models'),'[]')),
    'primary_cost_limit_usd',CASE WHEN plan_type='free' THEN json_extract(policy,'$.primary_cost_limit_usd')
        WHEN json_extract(quota,'$.primary_cost_limit_usd') IS NULL THEN json_extract(entitlement,'$.primary_cost_limit_usd')
        WHEN json_extract(entitlement,'$.primary_cost_limit_usd') IS NULL THEN json_extract(quota,'$.primary_cost_limit_usd')
        ELSE MIN(json_extract(quota,'$.primary_cost_limit_usd'),json_extract(entitlement,'$.primary_cost_limit_usd')) END,
    'weekly_cost_limit_usd',CASE WHEN plan_type='free' THEN json_extract(policy,'$.weekly_cost_limit_usd')
        WHEN json_extract(quota,'$.weekly_cost_limit_usd') IS NULL THEN json_extract(entitlement,'$.weekly_cost_limit_usd')
        WHEN json_extract(entitlement,'$.weekly_cost_limit_usd') IS NULL THEN json_extract(quota,'$.weekly_cost_limit_usd')
        ELSE MIN(json_extract(quota,'$.weekly_cost_limit_usd'),json_extract(entitlement,'$.weekly_cost_limit_usd')) END,
    'free_access_enabled',json(CASE WHEN json_extract(policy,'$.free_access_enabled') THEN 'true' ELSE 'false' END),
    'free_primary_cost_limit_usd',json_extract(policy,'$.primary_cost_limit_usd'),
    'free_weekly_cost_limit_usd',json_extract(policy,'$.weekly_cost_limit_usd')
) AS config FROM old;

INSERT INTO virtual_plans(id,name,plan_type,config)
SELECT 'legacy-'||MIN(m.id),m.plan_type||' · 迁移配置（'||substr(MIN(m.username),1,24)||'）',m.plan_type,m.config
FROM virtual_plan_migration m
WHERE NOT EXISTS(SELECT 1 FROM virtual_plans p WHERE p.plan_type=m.plan_type AND p.config=m.config)
GROUP BY m.plan_type,m.config;

UPDATE virtual_accounts SET plan_id=(
    SELECT p.id FROM virtual_plan_migration m JOIN virtual_plans p ON p.plan_type=m.plan_type AND p.config=m.config
    WHERE m.id=virtual_accounts.id LIMIT 1
);
DROP TABLE virtual_plan_migration;
CREATE INDEX virtual_accounts_plan ON virtual_accounts(plan_id);

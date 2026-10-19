-- Model names become the global catalog key with a platform label. Plans, API groups
-- and subscription routes switch to manual model rules and multi-tag supplier pools.

-- 1. Model catalog: one live row per model name. Administrators resolve genuine
-- duplicates before upgrading; shadowed tombstones carry no prices or policy.
DO $$
DECLARE duplicate TEXT;
BEGIN
    SELECT string_agg(model, ', ' ORDER BY model) INTO duplicate FROM (
        SELECT model FROM model_catalog WHERE deleted=FALSE GROUP BY model HAVING COUNT(*)>1
    ) d;
    IF duplicate IS NOT NULL THEN
        RAISE EXCEPTION 'Models configured on more than one platform must be reduced to one before upgrading: %', duplicate;
    END IF;
END;
$$;
DELETE FROM model_catalog c WHERE c.deleted=TRUE AND EXISTS(
    SELECT 1 FROM model_catalog o WHERE o.model=c.model AND o.provider_id<>c.provider_id
    AND (o.deleted=FALSE OR o.provider_id<c.provider_id));
DELETE FROM model_prices p WHERE NOT EXISTS(
    SELECT 1 FROM model_catalog c WHERE c.provider_id=p.provider_id AND c.model=p.model);
DELETE FROM model_image_prices p WHERE NOT EXISTS(
    SELECT 1 FROM model_catalog c WHERE c.provider_id=p.provider_id AND c.model=p.model);
INSERT INTO meta(key,value)
    SELECT regexp_replace(key,'^model_preset_pending:[a-z]+:','model_preset_pending:'),value
    FROM meta WHERE key ~ '^model_preset_pending:[a-z]+:'
    ON CONFLICT(key) DO NOTHING;
DELETE FROM meta WHERE key ~ '^model_preset_pending:[a-z]+:';
-- A disabled, never edited, unpriced row was only waiting for a verified price:
-- startup registration prices and enables it like any other pending built-in model.
INSERT INTO meta(key,value)
    SELECT 'model_preset_pending:'||c.model,'1' FROM model_catalog c
    WHERE c.deleted=FALSE AND c.enabled=FALSE AND c.revision=1
      AND NOT EXISTS(SELECT 1 FROM model_prices p WHERE p.provider_id=c.provider_id AND p.model=c.model)
      AND NOT EXISTS(SELECT 1 FROM model_image_prices p WHERE p.provider_id=c.provider_id AND p.model=c.model)
    ON CONFLICT(key) DO NOTHING;

DO $$
DECLARE c RECORD;
BEGIN
    FOR c IN SELECT conrelid::regclass AS tbl, conname FROM pg_constraint
        WHERE conrelid IN ('model_catalog'::regclass,'model_prices'::regclass,'model_image_prices'::regclass)
          AND contype='f'
    LOOP
        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', c.tbl, c.conname);
    END LOOP;
    FOR c IN SELECT conrelid::regclass AS tbl, conname FROM pg_constraint
        WHERE conrelid IN ('model_catalog'::regclass,'model_prices'::regclass,'model_image_prices'::regclass)
          AND contype='p'
    LOOP
        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', c.tbl, c.conname);
    END LOOP;
END;
$$;
ALTER TABLE model_catalog RENAME COLUMN provider_id TO platform;
ALTER TABLE model_catalog ADD PRIMARY KEY (model);
ALTER TABLE model_catalog ADD CONSTRAINT model_catalog_platform_check CHECK(length(platform) BETWEEN 1 AND 64);
ALTER TABLE model_prices DROP COLUMN provider_id;
ALTER TABLE model_prices ADD PRIMARY KEY (model,tier,min_input_tokens);
ALTER TABLE model_image_prices DROP COLUMN provider_id;
ALTER TABLE model_image_prices ADD PRIMARY KEY (model,resolution);
ALTER TABLE model_image_prices ADD FOREIGN KEY (model) REFERENCES model_catalog(model);

-- 2. One billing catalog revision for every platform.
DROP TABLE billing_cache_revisions;
CREATE TABLE billing_cache_revision (
    id BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK(id),
    revision BIGINT NOT NULL DEFAULT 0
);
INSERT INTO billing_cache_revision(id,revision) VALUES(TRUE,0);
CREATE OR REPLACE FUNCTION billing_catalog_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO billing_cache_revision(id,revision) VALUES(TRUE,1)
    ON CONFLICT(id) DO UPDATE SET revision=billing_cache_revision.revision+1;
    RETURN NULL;
END;
$$;

-- 3. Plans keep their client platform; the pool becomes a tag set and the model
-- scope an explicit list. "all" snapshots the platform's current non-deleted
-- models; "selected" keeps the listed catalog models. Entries carry no multiplier
-- or model-specific limits yet.
CREATE TABLE virtual_plan_tags (
    plan_id TEXT NOT NULL REFERENCES virtual_plans(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES supplier_tags(id) ON DELETE RESTRICT,
    PRIMARY KEY(plan_id,tag_id)
);
CREATE INDEX virtual_plan_tags_tag ON virtual_plan_tags(tag_id);
INSERT INTO virtual_plan_tags(plan_id,tag_id)
    SELECT p.id,p.config->>'supplier_tag_id' FROM virtual_plans p
    WHERE EXISTS(SELECT 1 FROM supplier_tags t WHERE t.id=p.config->>'supplier_tag_id');
UPDATE virtual_plans p SET config=(p.config - 'model_access' - 'supplier_tag_id' - 'models')
    || jsonb_build_object('models', COALESCE((
        SELECT jsonb_agg(jsonb_build_object('model',m.model,'multiplier',NULL::text,'concurrency_limit',0,'rpm_limit',0) ORDER BY m.model)
        FROM model_catalog m
        WHERE m.deleted=FALSE AND m.platform=p.provider_id
          AND ((p.config->>'model_access')='all'
               OR ((p.config->>'model_access')='selected' AND EXISTS(
                   SELECT 1 FROM jsonb_array_elements(p.config->'models') s
                   WHERE s->>'model'=m.model AND s->>'provider_id'=m.platform)))
    ),'[]'::jsonb)),
    revision=revision+1;

DROP VIEW effective_execution_routes;
DROP VIEW regular_subscription_pools;
DROP INDEX plans_supplier_tag;

-- 4. Subscription routes: an explicit override is a tag set; NULL inherits the plan.
ALTER TABLE execution_routes ADD COLUMN tag_ids JSONB CHECK(tag_ids IS NULL OR jsonb_typeof(tag_ids)='array');
UPDATE execution_routes SET tag_ids=jsonb_build_array(tag_id) WHERE tag_id IS NOT NULL;
DROP TRIGGER execution_route_provider_insert ON execution_routes;
DROP TRIGGER execution_route_provider_update ON execution_routes;
DROP FUNCTION execution_route_provider_insert_fn();
DROP FUNCTION execution_route_provider_update_fn();
DROP INDEX execution_route_tag;
ALTER TABLE execution_routes DROP COLUMN tag_id;
CREATE INDEX execution_route_tags ON execution_routes USING GIN (tag_ids);
CREATE FUNCTION execution_route_provider_insert_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NOT EXISTS(SELECT 1 FROM platform_accounts p WHERE p.id=NEW.virtual_account_id AND p.provider_id=NEW.provider_id)
 OR (NEW.tag_ids IS NOT NULL AND EXISTS(SELECT 1 FROM jsonb_array_elements_text(NEW.tag_ids) t WHERE NOT EXISTS(SELECT 1 FROM supplier_tags WHERE id=t))) THEN
RAISE EXCEPTION USING MESSAGE = 'execution route pool mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER execution_route_provider_insert BEFORE INSERT ON execution_routes
FOR EACH ROW EXECUTE FUNCTION execution_route_provider_insert_fn();
CREATE FUNCTION execution_route_provider_update_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.virtual_account_id!=OLD.virtual_account_id OR NEW.provider_id!=OLD.provider_id
 OR (NEW.tag_ids IS NOT NULL AND EXISTS(SELECT 1 FROM jsonb_array_elements_text(NEW.tag_ids) t WHERE NOT EXISTS(SELECT 1 FROM supplier_tags WHERE id=t))) THEN
RAISE EXCEPTION USING MESSAGE = 'execution route pool mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;
CREATE TRIGGER execution_route_provider_update BEFORE UPDATE ON execution_routes
FOR EACH ROW EXECUTE FUNCTION execution_route_provider_update_fn();
CREATE VIEW regular_subscription_pools AS SELECT a.id AS virtual_account_id,a.provider_id,p.id AS plan_id,COALESCE((SELECT jsonb_agg(t.tag_id ORDER BY t.tag_id) FROM virtual_plan_tags t WHERE t.plan_id=p.id),'[]'::jsonb) AS tag_ids FROM regular_platforms a JOIN virtual_plans p ON p.id=a.plan_id;
CREATE VIEW effective_execution_routes AS SELECT r.virtual_account_id,r.provider_id,r.revision,COALESCE(r.tag_ids,p.tag_ids) AS tag_ids FROM execution_routes r JOIN regular_subscription_pools p ON p.virtual_account_id=r.virtual_account_id;

-- 5. API groups: the allowed list and per-model rules merge into one manual model list.
-- An empty allowed list snapshots the platform's current non-deleted models.
ALTER TABLE api_groups ADD COLUMN models JSONB NOT NULL DEFAULT '[]' CHECK(jsonb_typeof(models)='array');
UPDATE api_groups g SET models=COALESCE((
    SELECT jsonb_agg(jsonb_build_object('model',m.model,'multiplier',r.multiplier,'concurrency_limit',COALESCE(r.concurrency_limit,0),'rpm_limit',COALESCE(r.rpm_limit,0)) ORDER BY m.model)
    FROM model_catalog m
    LEFT JOIN LATERAL (
        SELECT rule->'multiplier' AS multiplier,(rule->>'concurrency_limit')::bigint AS concurrency_limit,(rule->>'rpm_limit')::bigint AS rpm_limit
        FROM jsonb_array_elements(g.model_rules) rule WHERE rule->>'model'=m.model LIMIT 1
    ) r ON TRUE
    WHERE m.deleted=FALSE AND m.platform=g.provider_id
      AND (jsonb_array_length(g.allowed_models)=0 OR g.allowed_models ? m.model)
),'[]'::jsonb), revision=revision+1
WHERE g.deleted=FALSE;
DROP TRIGGER api_group_provider_immutable ON api_groups;
DROP FUNCTION api_group_provider_immutable_fn();
DROP TRIGGER api_group_tag_provider ON api_group_tags;
DROP FUNCTION api_group_tag_provider_fn();
DO $$
DECLARE constraint_name TEXT;
BEGIN
    FOR constraint_name IN SELECT conname FROM pg_constraint
        WHERE conrelid='api_groups'::regclass AND contype='c'
          AND pg_get_constraintdef(oid) LIKE '%search_price_nano_usd%'
    LOOP
        EXECUTE format('ALTER TABLE api_groups DROP CONSTRAINT %I',constraint_name);
    END LOOP;
END;
$$;
ALTER TABLE api_groups DROP COLUMN provider_id, DROP COLUMN allowed_models, DROP COLUMN model_rules;
ALTER TABLE api_groups ADD CONSTRAINT api_groups_search_price_check CHECK(search_price_nano_usd IS NULL OR search_price_nano_usd>=0);

-- 6. API usage ownership no longer compares the group's platform.
CREATE OR REPLACE FUNCTION usage_owner_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF (NEW.user_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM regular_users WHERE id=NEW.user_id))
 OR (NEW.subject_kind='api_key' AND NOT EXISTS(SELECT 1 FROM api_keys k JOIN api_groups g ON g.id=k.group_id
     WHERE k.id=NEW.key_id AND k.id=NEW.subject_id AND k.user_id=NEW.user_id AND g.id=NEW.group_id))
 OR (NEW.subject_kind='virtual_account' AND NEW.user_id IS NOT NULL AND NOT EXISTS(
     SELECT 1 FROM regular_platforms WHERE id=NEW.subject_id AND user_id=NEW.user_id AND provider_id=NEW.provider_id)) THEN
RAISE EXCEPTION USING MESSAGE = 'usage ownership mismatch', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;

-- 7. Platform labels name the model's vendor. Antigravity is a supplier channel,
-- not a vendor: the models it serves keep their vendor label, and a model whose
-- name reveals no vendor leaves the billing catalog together with its prices and
-- API group rules. It stays in the supplier account's available models and can be
-- added to the catalog manually. Runs after the snapshots above, which matched
-- models by their former supplier platform.
UPDATE model_catalog SET platform='claude' WHERE platform='antigravity' AND lower(model) LIKE 'claude%';
UPDATE model_catalog SET platform='gemini' WHERE platform='antigravity' AND lower(model) LIKE 'gemini%';
UPDATE model_catalog SET platform='chatgpt' WHERE platform='antigravity' AND lower(model) LIKE 'gpt%';
UPDATE model_catalog SET platform='grok' WHERE platform='antigravity' AND lower(model) LIKE 'grok%';
DELETE FROM meta WHERE key IN (SELECT 'model_preset_pending:'||model FROM model_catalog WHERE platform='antigravity');
DELETE FROM model_prices WHERE model IN (SELECT model FROM model_catalog WHERE platform='antigravity');
DELETE FROM model_image_prices WHERE model IN (SELECT model FROM model_catalog WHERE platform='antigravity');
UPDATE api_groups g SET models=COALESCE((
        SELECT jsonb_agg(e ORDER BY e->>'model') FROM jsonb_array_elements(g.models) e
        WHERE NOT EXISTS(SELECT 1 FROM model_catalog m WHERE m.model=e->>'model' AND m.platform='antigravity')
    ),'[]'::jsonb), revision=revision+1
    WHERE EXISTS(SELECT 1 FROM jsonb_array_elements(g.models) e
        JOIN model_catalog m ON m.model=e->>'model' WHERE m.platform='antigravity');
DELETE FROM model_catalog WHERE platform='antigravity';

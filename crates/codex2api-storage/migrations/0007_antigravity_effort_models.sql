-- Antigravity reasoning-effort variants are one model each: an account lists the base
-- model with the upstream IDs grouped under it, and discovery never writes the billing
-- catalog. Built-in Claude models take Anthropic's model IDs, which Antigravity serves.

-- 1. Upstream models of Antigravity accounts and the base model each one belongs to;
-- the effort suffixes are -extra-low, -extra-high, -low, -medium and -high.
CREATE TEMP TABLE antigravity_models ON COMMIT DROP AS
SELECT m.account_id, m.model AS upstream, m.observed_at,
    CASE WHEN m.model ~ '.-extra-(low|high)$' THEN regexp_replace(m.model,'-extra-(low|high)$','')
         WHEN m.model ~ '.-(low|medium|high)$' THEN regexp_replace(m.model,'-(low|medium|high)$','')
         ELSE m.model END AS base
FROM supplier_models m JOIN supplier_accounts a ON a.id=m.account_id
WHERE a.provider_id='antigravity' AND m.source='upstream';

-- 2. Each base model replaces its variants in the account's model list. Variant
-- cooldowns no longer name a routed model.
DELETE FROM supplier_models m USING antigravity_models v
    WHERE m.account_id=v.account_id AND m.model=v.upstream;
INSERT INTO supplier_models(account_id,model,source,descriptor_json,observed_at)
SELECT account_id, base, 'upstream',
    jsonb_build_object('id',base,'model',base,'object','model',
        'upstream_models',jsonb_agg(upstream ORDER BY upstream COLLATE "C"))::text,
    max(observed_at)
FROM antigravity_models GROUP BY account_id, base
ON CONFLICT(account_id,model) DO UPDATE
    SET descriptor_json=excluded.descriptor_json, observed_at=excluded.observed_at;
DELETE FROM supplier_model_cooldowns c USING antigravity_models v
    WHERE c.account_id=v.account_id AND c.model=v.upstream AND v.base<>v.upstream;

-- 3. Dotted built-in Claude names and catalog rows named after an effort variant
-- merge into the model they name. An existing row under the new name survives;
-- otherwise the most deliberate old row (live, built-in, enabled, most edited)
-- takes the new name together with its prices.
CREATE TEMP TABLE model_renames(
    former TEXT PRIMARY KEY,
    target TEXT NOT NULL,
    priority INTEGER NOT NULL
) ON COMMIT DROP;
INSERT INTO model_renames(former,target,priority) VALUES
    ('claude-fable-5.1','claude-fable-5-1',0),
    ('claude-opus-5.5','claude-opus-5-5',0),
    ('claude-opus-4.8','claude-opus-4-8',0),
    ('claude-opus-4.7','claude-opus-4-7',0),
    ('claude-opus-4.6','claude-opus-4-6',0),
    ('claude-opus-4.5','claude-opus-4-5',0),
    ('claude-sonnet-5.5','claude-sonnet-5-5',0),
    ('claude-sonnet-4.6','claude-sonnet-4-6',0),
    ('claude-sonnet-4.5','claude-sonnet-4-5',0),
    ('claude-haiku-5.5','claude-haiku-5-5',0),
    ('claude-haiku-4.5','claude-haiku-4-5',0);
INSERT INTO model_renames(former,target,priority)
    SELECT DISTINCT upstream, base, 1 FROM antigravity_models WHERE base<>upstream
    ON CONFLICT(former) DO NOTHING;
CREATE TEMP TABLE model_merges ON COMMIT DROP AS
SELECT DISTINCT ON (r.target) r.target, r.former AS keep
FROM model_renames r JOIN model_catalog c ON c.model=r.former
WHERE NOT EXISTS(SELECT 1 FROM model_catalog e WHERE e.model=r.target)
ORDER BY r.target, c.deleted, r.priority, c.enabled DESC, c.revision DESC, r.former;

DELETE FROM model_prices p USING model_renames r
    WHERE p.model=r.former AND NOT EXISTS(SELECT 1 FROM model_merges k WHERE k.keep=r.former);
DELETE FROM model_image_prices p USING model_renames r
    WHERE p.model=r.former AND NOT EXISTS(SELECT 1 FROM model_merges k WHERE k.keep=r.former);
DELETE FROM model_catalog c USING model_renames r
    WHERE c.model=r.former AND NOT EXISTS(SELECT 1 FROM model_merges k WHERE k.keep=r.former);
INSERT INTO model_catalog(model,platform,kind,enabled,deleted,revision)
    SELECT k.target, c.platform, c.kind, c.enabled, c.deleted, c.revision
    FROM model_merges k JOIN model_catalog c ON c.model=k.keep;
DELETE FROM model_prices p USING model_merges k WHERE p.model=k.target;
UPDATE model_prices p SET model=k.target FROM model_merges k WHERE p.model=k.keep;
UPDATE model_image_prices p SET model=k.target FROM model_merges k WHERE p.model=k.keep;
DELETE FROM model_catalog c USING model_merges k WHERE c.model=k.keep;
INSERT INTO meta(key,value)
    SELECT 'model_preset_pending:'||k.target, m.value
    FROM model_merges k JOIN meta m ON m.key='model_preset_pending:'||k.keep
    ON CONFLICT(key) DO NOTHING;
DELETE FROM meta m USING model_renames r WHERE m.key='model_preset_pending:'||r.former;

-- 4. Plans and API groups name the merged models with one rule per model: the rule
-- that already names it, otherwise the first rule renamed into it.
UPDATE api_groups g SET models=(
    SELECT COALESCE(jsonb_agg(d.entry ORDER BY d.ord),'[]'::jsonb) FROM (
        SELECT DISTINCT ON (COALESCE(r.target,x.entry->>'model'))
            CASE WHEN r.target IS NULL THEN x.entry
                 ELSE jsonb_set(x.entry,'{model}',to_jsonb(r.target)) END AS entry,
            x.ord
        FROM jsonb_array_elements(g.models) WITH ORDINALITY AS x(entry,ord)
        LEFT JOIN model_renames r ON r.former=x.entry->>'model'
        ORDER BY COALESCE(r.target,x.entry->>'model'), r.target IS NOT NULL, r.priority, x.ord
    ) d
), revision=revision+1
WHERE EXISTS(SELECT 1 FROM jsonb_array_elements(g.models) e
    JOIN model_renames r ON r.former=e->>'model');
UPDATE virtual_plans p SET config=jsonb_set(p.config,'{models}',(
    SELECT COALESCE(jsonb_agg(d.entry ORDER BY d.ord),'[]'::jsonb) FROM (
        SELECT DISTINCT ON (COALESCE(r.target,x.entry->>'model'))
            CASE WHEN r.target IS NULL THEN x.entry
                 ELSE jsonb_set(x.entry,'{model}',to_jsonb(r.target)) END AS entry,
            x.ord
        FROM jsonb_array_elements(p.config->'models') WITH ORDINALITY AS x(entry,ord)
        LEFT JOIN model_renames r ON r.former=x.entry->>'model'
        ORDER BY COALESCE(r.target,x.entry->>'model'), r.target IS NOT NULL, r.priority, x.ord
    ) d
)), revision=revision+1
WHERE EXISTS(SELECT 1 FROM jsonb_array_elements(
        CASE WHEN jsonb_typeof(p.config->'models')='array' THEN p.config->'models' ELSE '[]'::jsonb END) e
    JOIN model_renames r ON r.former=e->>'model');

-- A fresh database applies its migrations in one transaction; drop this file's work tables.
DROP TABLE antigravity_models, model_renames, model_merges;

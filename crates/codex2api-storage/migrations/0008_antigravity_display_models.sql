-- Antigravity models follow the display names Antigravity reports: upstream IDs whose
-- display name reads the same once a trailing "(Low)", "(Medium)" or "(High)" is
-- removed are one model, and that label, not the ID, names the reasoning effort. The
-- client-facing name is the display name in lowercase with hyphens (Gemini 3.5 Flash
-- is gemini-3.5-flash), so built-in Claude models return to that form.

-- 1. Each Antigravity account's latest official model list, grouped like discovery:
-- the label selects an effort, and of several IDs for one label the ID equal to the
-- model name (with "-effort" for a label) is kept, otherwise the first in byte order.
-- Anthropic and OpenAI models share the quota pool "anthropic-openai".
CREATE TEMP TABLE antigravity_listed ON COMMIT DROP AS
SELECT q.account_id, q.observed_at, e.key AS upstream,
    COALESCE(NULLIF(trim(e.value->>'displayName'),''), e.key) AS display,
    e.value->>'modelProvider' AS provider
FROM account_quota_cache q
JOIN supplier_accounts a ON a.id=q.account_id AND a.provider_id='antigravity'
CROSS JOIN LATERAL jsonb_each(
    CASE WHEN jsonb_typeof(q.response_json::jsonb->'models')='object'
         THEN q.response_json::jsonb->'models' ELSE '{}'::jsonb END) e
WHERE e.key ~ '^[A-Za-z0-9._:/-]+$' AND length(e.key)<=256;
CREATE TEMP TABLE antigravity_grouped ON COMMIT DROP AS
SELECT account_id, observed_at, upstream, provider, lower(label[2]) AS effort,
    COALESCE(label[1], display) AS name,
    COALESCE(NULLIF(trim(both '-' from regexp_replace(regexp_replace(
        lower(COALESCE(label[1], display)), '[^a-z0-9._[:space:]-]', '', 'g'),
        '[[:space:]-]+', '-', 'g')), ''), upstream) AS model
FROM (SELECT l.*, regexp_match(l.display,
        '^(.*[^[:space:]])[[:space:]]*\([[:space:]]*(low|medium|high)[[:space:]]*\)$', 'i') AS label
      FROM antigravity_listed l) p;
CREATE TEMP TABLE antigravity_choices ON COMMIT DROP AS
SELECT DISTINCT ON (account_id, model, COALESCE(effort,'')) account_id, model, effort, upstream
FROM antigravity_grouped
ORDER BY account_id, model, COALESCE(effort,''),
    upstream=CASE WHEN effort IS NULL THEN model ELSE model||'-'||effort END DESC,
    upstream COLLATE "C";

-- 2. Models grouped by ID suffix are replaced: accounts with a stored model list get
-- their models back from it, the others on the next upstream model sync. Model
-- cooldowns of the old names no longer apply.
DELETE FROM supplier_models m USING supplier_accounts a
    WHERE a.id=m.account_id AND a.provider_id='antigravity' AND m.source='upstream';
DELETE FROM supplier_model_cooldowns c USING supplier_accounts a
    WHERE a.id=c.account_id AND a.provider_id='antigravity';
INSERT INTO supplier_models(account_id,model,source,descriptor_json,observed_at)
SELECT g.account_id, g.model, 'upstream',
    (jsonb_build_object('id',g.model,'model',g.model,'object','model','name',g.name,
        'efforts',COALESCE((SELECT jsonb_object_agg(c.effort,c.upstream) FROM antigravity_choices c
            WHERE c.account_id=g.account_id AND c.model=g.model AND c.effort IS NOT NULL),'{}'::jsonb))
     || COALESCE((SELECT jsonb_build_object('upstream_model',c.upstream) FROM antigravity_choices c
            WHERE c.account_id=g.account_id AND c.model=g.model AND c.effort IS NULL),'{}'::jsonb)
     || CASE WHEN g.pooled THEN jsonb_build_object('quota_pool','anthropic-openai')
             ELSE '{}'::jsonb END)::text,
    to_char(g.observed_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS"Z"')
FROM (SELECT account_id, model, max(observed_at) AS observed_at,
        (array_agg(name ORDER BY upstream COLLATE "C"))[1] AS name,
        bool_or(provider IN ('MODEL_PROVIDER_ANTHROPIC','MODEL_PROVIDER_OPENAI')) AS pooled
      FROM antigravity_grouped GROUP BY account_id, model) g
ON CONFLICT(account_id,model) DO UPDATE
    SET descriptor_json=excluded.descriptor_json, observed_at=excluded.observed_at;

-- 3. Catalog rows merge into the model they now name: the Claude IDs 0007 introduced,
-- and rows named after an upstream ID that belongs to another model (other than
-- built-in models). An existing row under the new name survives; otherwise the most
-- deliberate old row (live, Claude rename first, enabled, most edited) takes the new
-- name together with its prices.
CREATE TEMP TABLE model_renames(
    former TEXT PRIMARY KEY,
    target TEXT NOT NULL,
    priority INTEGER NOT NULL
) ON COMMIT DROP;
INSERT INTO model_renames(former,target,priority) VALUES
    ('claude-fable-5-1','claude-fable-5.1',0),
    ('claude-opus-5-5','claude-opus-5.5',0),
    ('claude-opus-4-8','claude-opus-4.8',0),
    ('claude-opus-4-7','claude-opus-4.7',0),
    ('claude-opus-4-6','claude-opus-4.6',0),
    ('claude-opus-4-5','claude-opus-4.5',0),
    ('claude-sonnet-5-5','claude-sonnet-5.5',0),
    ('claude-sonnet-4-6','claude-sonnet-4.6',0),
    ('claude-sonnet-4-5','claude-sonnet-4.5',0),
    ('claude-haiku-5-5','claude-haiku-5.5',0),
    ('claude-haiku-4-5','claude-haiku-4.5',0);
INSERT INTO model_renames(former,target,priority)
    SELECT upstream, model, 1 FROM (
    SELECT DISTINCT ON (g.upstream) g.upstream, g.model
    FROM antigravity_grouped g
    WHERE g.model<>g.upstream
      AND NOT EXISTS(SELECT 1 FROM antigravity_grouped o WHERE o.model=g.upstream)
      AND g.upstream NOT IN ('gpt-6-astra','gpt-6.1-sol','gpt-6-sol','gpt-6-luna','gpt-5.6-sol',
        'gpt-5.6-terra','gpt-5.6-luna','gpt-5.5','grok-4.7','grok-build-0.1','grok-4.6',
        'grok-4.5','grok-4.3','grok-4.20-multi-agent-0309','grok-4.20-0309-reasoning',
        'grok-4.20-0309-non-reasoning','gemini-4-argon','gemini-3.8-flash','gemini-3.6-flash',
        'gemini-3.5-flash-lite','gemini-3.1-pro','gemini-3.1-flash-lite','gemini-3-flash',
        'gemini-2.5-pro','gemini-2.5-flash','gemini-2.5-flash-lite','claude-fable-5.1',
        'claude-fable-5','claude-opus-5.5','claude-opus-5','claude-opus-4.8','claude-opus-4.7',
        'claude-opus-4.6','claude-opus-4.5','claude-sonnet-5.5','claude-sonnet-5',
        'claude-sonnet-4.6','claude-sonnet-4.5','claude-haiku-5.5','claude-haiku-4.5')
    ORDER BY g.upstream, g.model) s
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
DROP TABLE antigravity_listed, antigravity_grouped, antigravity_choices, model_renames, model_merges;

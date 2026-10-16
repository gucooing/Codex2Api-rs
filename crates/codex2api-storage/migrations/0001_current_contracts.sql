-- Executed once by Storage::initialize inside its schema-locked transaction.
DO $$
DECLARE settings JSONB;
BEGIN
    SELECT value::jsonb INTO settings FROM meta WHERE key='public_url_settings' FOR UPDATE;
    IF settings ? 'metered_api_url' THEN
        IF settings ? 'oauth_url'
            OR NULLIF(btrim(settings->>'api_url'), '') IS NULL
            OR NULLIF(btrim(settings->>'metered_api_url'), '') IS NULL THEN
            RAISE EXCEPTION 'Public service origins are incomplete or ambiguous';
        END IF;
        UPDATE meta SET value=((settings - 'api_url' - 'metered_api_url') || jsonb_build_object(
            'oauth_url', settings->'api_url', 'api_url', settings->'metered_api_url',
            'revision', (settings->>'revision')::bigint + 1))::text
        WHERE key='public_url_settings';
    END IF;
END;
$$;

ALTER TABLE supplier_accounts DROP COLUMN IF EXISTS home_dir;

-- Reuse persisted identity only; never generate device or credential data.
UPDATE supplier_accounts SET http_fingerprint_json=jsonb_build_object(
    'originator',originator,'user_agent',user_agent,'os_type',os_type,
    'os_version',os_version,'arch',arch,'installation_id',installation_id,'cookies','[]'::jsonb)::text
WHERE provider_id='chatgpt' AND btrim(http_fingerprint_json) IN ('','{}');

UPDATE supplier_accounts SET user_agent=regexp_replace(user_agent,'^codex_cli_rs/[^ ]+','codex_cli_rs/0.162.1'),
    http_fingerprint_json=jsonb_set(http_fingerprint_json::jsonb,'{user_agent}',
        to_jsonb(regexp_replace(http_fingerprint_json::jsonb->>'user_agent','^codex_cli_rs/[^ ]+','codex_cli_rs/0.162.1')))::text
WHERE provider_id='chatgpt' AND user_agent ~ '^codex_cli_rs/[^ ]+ '
    AND http_fingerprint_json::jsonb->>'user_agent' ~ '^codex_cli_rs/[^ ]+ ';

UPDATE supplier_accounts SET http_fingerprint_json=(jsonb_build_object(
    'account_id',id,'installation_id',installation_id,'originator',originator,
    'user_agent',user_agent,'os_type',os_type,'os_version',os_version,'arch',arch,'timezone',NULL)
    || CASE WHEN btrim(http_fingerprint_json)='' THEN '{}'::jsonb ELSE http_fingerprint_json::jsonb END)::text
WHERE provider_id='grok';

UPDATE supplier_tokens t SET raw_auth_json=jsonb_build_object(
    'auth_mode',t.auth_mode,'tokens',jsonb_build_object('id_token',COALESCE(t.id_token,''),
    'access_token',COALESCE(t.access_token,''),'refresh_token',COALESCE(t.refresh_token,''),
    'account_id',a.chatgpt_account_id),'last_refresh',t.last_refresh)::text
FROM supplier_accounts a WHERE a.id=t.account_id AND a.provider_id='chatgpt'
    AND NULLIF(btrim(t.raw_auth_json),'') IS NULL;

-- Local model groups use the Desktop's display_text/slugs contract.
UPDATE virtual_client_state s SET value_json=jsonb_set(s.value_json,'{versions}',(
    SELECT COALESCE(jsonb_agg((v - 'name' - 'models') || jsonb_build_object(
        'id',COALESCE(NULLIF(v->>'id',''),gen_random_uuid()::text),
        'display_text',COALESCE(NULLIF(v->'display_text','null'::jsonb),v->'name'),
        'slugs',COALESCE(NULLIF(v->'slugs','null'::jsonb),v->'models','[]'::jsonb),
        'intelligence_presets',COALESCE(NULLIF(v->'intelligence_presets','null'::jsonb),'[]'::jsonb)) ORDER BY position),'[]'::jsonb)
    FROM jsonb_array_elements(s.value_json->'versions') WITH ORDINALITY AS versions(v,position)
)), revision=revision+1
WHERE state_key='models' AND jsonb_typeof(value_json->'versions')='array'
    AND EXISTS(SELECT 1 FROM jsonb_array_elements(value_json->'versions') v
        WHERE v ? 'name' OR v ? 'models' OR NULLIF(v->>'id','') IS NULL
            OR v->'intelligence_presets' IS NULL OR v->'intelligence_presets'='null'::jsonb);

UPDATE virtual_plans SET plan_type=CASE plan_type WHEN 'premium' THEN 'x_premium'
    WHEN 'premium_plus' THEN 'x_premium_plus' WHEN 'supergrok_pro' THEN 'supergrok_heavy' END,
    revision=revision+1
WHERE provider_id='grok' AND plan_type IN ('premium','premium_plus','supergrok_pro');
UPDATE platform_accounts a SET plan_type=p.plan_type
FROM virtual_plans p WHERE a.plan_id=p.id AND a.provider_id=p.provider_id AND a.plan_type<>p.plan_type;

DO $$
DECLARE constraint_name TEXT;
BEGIN
    FOR constraint_name IN SELECT conname FROM pg_constraint
        WHERE conrelid='virtual_plans'::regclass AND contype='c'
          AND pg_get_constraintdef(oid) LIKE '%provider_id%'
          AND pg_get_constraintdef(oid) LIKE '%plan_type%'
    LOOP
        EXECUTE format('ALTER TABLE virtual_plans DROP CONSTRAINT %I',constraint_name);
    END LOOP;
END;
$$;
ALTER TABLE virtual_plans ADD CONSTRAINT virtual_plans_provider_tier CHECK(
    (provider_id='chatgpt' AND plan_type IN ('free','go','plus','prolite','pro','promax','team','business','enterprise','edu','edu_plus','edu_pro','self_serve_business_prolite','self_serve_business_usage_based','ent26','enterprise_cbp_automation','enterprise_cbp_usage_based'))
    OR (provider_id='grok' AND plan_type IN ('free','x_basic','x_premium','x_premium_plus','supergrok_lite','supergrok','supergrok_plus','supergrok_heavy')));

-- Normalize ledger metadata without changing any measured usage, prices, charges,
-- balances, ownership IDs or timestamps. Restore immutable guards in this transaction.
LOCK TABLE usage_records IN ACCESS EXCLUSIVE MODE;
ALTER TABLE usage_records DISABLE TRIGGER usage_identity_immutable;
ALTER TABLE usage_records DISABLE TRIGGER usage_source_name_immutable;
UPDATE usage_records SET subject_kind='api_key' WHERE subject_kind='retired_api_key';
UPDATE usage_records SET source_name=group_name WHERE source_name IS NULL AND group_name IS NOT NULL;
UPDATE usage_records SET billing_status='unpriced' WHERE billing_status='legacy';
WITH snapshots AS (
    SELECT id,provider_id,CASE WHEN jsonb_typeof(pricing_snapshot_json::jsonb)='array'
        THEN jsonb_build_object('tokens',pricing_snapshot_json::jsonb,'images','[]'::jsonb,'operations','[]'::jsonb)
        ELSE pricing_snapshot_json::jsonb END AS snapshot
    FROM usage_records WHERE pricing_snapshot_json IS NOT NULL
), converted AS (
    SELECT id,snapshot || jsonb_build_object(
        'tokens',(SELECT COALESCE(jsonb_agg(p || jsonb_build_object('provider_id',COALESCE(p->>'provider_id',provider_id)) ORDER BY n),'[]'::jsonb)
            FROM jsonb_array_elements(snapshot->'tokens') WITH ORDINALITY prices(p,n)),
        'images',(SELECT COALESCE(jsonb_agg(p || jsonb_build_object('provider_id',COALESCE(p->>'provider_id',provider_id)) ORDER BY n),'[]'::jsonb)
            FROM jsonb_array_elements(snapshot->'images') WITH ORDINALITY prices(p,n)),
        'operations',COALESCE(snapshot->'operations','[]'::jsonb)) AS snapshot
    FROM snapshots
)
UPDATE usage_records u SET pricing_snapshot_json=c.snapshot::text FROM converted c
WHERE c.id=u.id AND u.pricing_snapshot_json::jsonb IS DISTINCT FROM c.snapshot;
ALTER TABLE usage_records ENABLE TRIGGER usage_source_name_immutable;
ALTER TABLE usage_records ENABLE TRIGGER usage_identity_immutable;

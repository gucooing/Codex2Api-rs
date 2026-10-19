-- Preserve frozen account inputs once; provider versions and URLs live in code.
DO $$
DECLARE account RECORD; fingerprint JSONB; terminal TEXT; prefix TEXT;
BEGIN
    FOR account IN SELECT * FROM supplier_accounts ORDER BY id FOR UPDATE LOOP
        fingerprint := account.http_fingerprint_json::jsonb;
        IF jsonb_typeof(fingerprint) IS DISTINCT FROM 'object'
            OR fingerprint->>'installation_id' IS DISTINCT FROM account.installation_id
            OR fingerprint->>'os_type' IS DISTINCT FROM account.os_type
            OR fingerprint->>'os_version' IS DISTINCT FROM account.os_version
            OR fingerprint->>'arch' IS DISTINCT FROM account.arch THEN
            RAISE EXCEPTION 'Supplier % has inconsistent frozen identity', account.id;
        END IF;
        IF account.provider_id = 'chatgpt' THEN
            prefix := substring(account.user_agent from '^codex_cli_rs/[^ ]+ ')
                || '(' || account.os_type || ' ' || account.os_version || '; ' || account.arch || ') ';
            IF prefix IS NULL OR NOT starts_with(account.user_agent, prefix)
                OR fingerprint->>'user_agent' IS DISTINCT FROM account.user_agent
                OR account.originator IS DISTINCT FROM 'codex_cli_rs'
                OR fingerprint->>'originator' IS DISTINCT FROM account.originator THEN
                RAISE EXCEPTION 'Supplier % has invalid ChatGPT fingerprint', account.id;
            END IF;
            terminal := substr(account.user_agent, length(prefix) + 1);
            IF btrim(terminal) = '' OR octet_length(terminal) > 256 OR terminal ~ '[^ -~]' THEN
                RAISE EXCEPTION 'Supplier % has invalid terminal identity', account.id;
            END IF;
            fingerprint := (fingerprint - 'originator' - 'user_agent')
                || jsonb_build_object('terminal', terminal);
        ELSIF account.provider_id = 'grok' THEN
            fingerprint := fingerprint - 'originator' - 'user_agent';
        ELSE
            RAISE EXCEPTION 'Supplier % has unsupported provider', account.id;
        END IF;
        UPDATE supplier_accounts SET http_fingerprint_json = fingerprint::text WHERE id = account.id;
    END LOOP;
END;
$$;

ALTER TABLE supplier_accounts DROP COLUMN originator, DROP COLUMN user_agent;
DELETE FROM meta WHERE key IN ('codex_ref_commit', 'codex_ref_repo');

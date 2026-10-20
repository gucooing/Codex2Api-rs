-- API-key accounts: the OpenAI wires accept only a Bearer token, and every account
-- carries its same-account retry count. A format change is not a reauthorization, so
-- the credential revision, and the blocks observed on it, stay.
ALTER TABLE supplier_tokens DISABLE TRIGGER supplier_auth_update;
UPDATE supplier_tokens t SET raw_auth_json = (
        t.raw_auth_json::jsonb
        || jsonb_build_object('retry_count', 2)
        || CASE WHEN t.raw_auth_json::jsonb->>'wire' = 'messages' THEN '{}'::jsonb
                ELSE jsonb_build_object('auth_scheme', 'bearer') END
    )::text
FROM supplier_accounts a
WHERE a.id = t.account_id AND a.provider_id = 'api';
ALTER TABLE supplier_tokens ENABLE TRIGGER supplier_auth_update;

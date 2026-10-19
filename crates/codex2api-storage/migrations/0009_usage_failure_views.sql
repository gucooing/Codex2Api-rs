-- The administrator sees the real failure of a request and every supplier attempt the
-- gateway recovered from; the consumer sees only the outcome of that handling.
-- account_provider_id is the channel of the supplier account that served the final
-- attempt, and public_error_* is the failure the consumer received.
ALTER TABLE usage_records ADD COLUMN account_provider_id TEXT;
ALTER TABLE usage_records ADD COLUMN public_error_code TEXT;
ALTER TABLE usage_records ADD COLUMN public_error_message TEXT;

CREATE TABLE usage_attempts (
    usage_record_id TEXT NOT NULL REFERENCES usage_records(id) ON DELETE RESTRICT,
    attempt INTEGER NOT NULL CHECK(attempt>=1),
    account_id TEXT NOT NULL,
    account_name TEXT NOT NULL,
    account_provider_id TEXT NOT NULL,
    upstream_endpoint TEXT,
    http_status BIGINT,
    error_code TEXT,
    error_message TEXT,
    recovery TEXT NOT NULL CHECK(recovery IN ('refresh','failover')),
    started_at_ms BIGINT NOT NULL,
    total_ms BIGINT NOT NULL CHECK(total_ms>=0),
    PRIMARY KEY(usage_record_id,attempt)
);

CREATE FUNCTION usage_attempt_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
RAISE EXCEPTION USING MESSAGE = 'usage attempts are immutable', ERRCODE = '23514';
END;
$$;
CREATE TRIGGER usage_attempt_immutable BEFORE UPDATE OR DELETE ON usage_attempts
FOR EACH ROW EXECUTE FUNCTION usage_attempt_immutable_fn();

-- Existing measurements, charges and ownership are unchanged. The serving channel is
-- read from supplier accounts that still exist, and the Antigravity wire path becomes
-- the canonical Responses path every ledger row records. Channels of deleted suppliers,
-- earlier failed attempts and the errors earlier consumers received remain unknown.
LOCK TABLE usage_records IN ACCESS EXCLUSIVE MODE;
ALTER TABLE usage_records DISABLE TRIGGER usage_identity_immutable;
UPDATE usage_records u SET account_provider_id=s.provider_id
FROM supplier_accounts s WHERE s.id=u.account_id;
UPDATE usage_records SET upstream_endpoint='/v1/responses'
WHERE upstream_endpoint='/v1internal:streamGenerateContent';
ALTER TABLE usage_records ENABLE TRIGGER usage_identity_immutable;

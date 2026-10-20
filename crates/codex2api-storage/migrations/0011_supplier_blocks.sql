-- Supplier availability is a set of blocks judged by the supplier channel: an
-- account-wide or model-scoped avoidance that ends at a time or never. One active
-- block per (account, scope, model); a permanent block outranks a timed one and a
-- later expiry outranks an earlier one. Blocks carry the credential revision they
-- were observed on so a late failure cannot block freshly replaced credentials, and a
-- successful reauthorization deletes every block of the account.
CREATE TABLE supplier_blocks (
    account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK(scope IN ('account','model')),
    model TEXT NOT NULL DEFAULT '' CHECK((scope='account')=(model='')),
    kind TEXT NOT NULL CHECK(kind IN ('auth','billing','rate_limit','quota','overload','transport','model_missing','penalty')),
    until_at BIGINT CHECK(until_at IS NULL OR until_at>0),
    code TEXT,
    message TEXT,
    observed_at BIGINT NOT NULL,
    auth_revision BIGINT NOT NULL,
    PRIMARY KEY(account_id,scope,model)
);
CREATE INDEX supplier_blocks_until ON supplier_blocks(until_at);

-- Existing credential rejections, billing rejections, quota cooldowns and model
-- cooldowns become blocks once; a credential rejection outranks the others.
INSERT INTO supplier_blocks(account_id,scope,model,kind,until_at,code,message,observed_at,auth_revision)
SELECT h.account_id,'account','','auth',NULL,NULL,h.error_message,
       floor(extract(epoch FROM clock_timestamp()))::bigint,a.auth_revision
FROM supplier_health h JOIN supplier_accounts a ON a.id=h.account_id
WHERE h.rejected_auth_revision=a.auth_revision;
INSERT INTO supplier_blocks(account_id,scope,model,kind,until_at,code,message,observed_at,auth_revision)
SELECT h.account_id,'account','','billing',NULL,h.payment_required_code,NULL,
       floor(extract(epoch FROM clock_timestamp()))::bigint,a.auth_revision
FROM supplier_health h JOIN supplier_accounts a ON a.id=h.account_id
WHERE h.payment_required_at IS NOT NULL
ON CONFLICT(account_id,scope,model) DO NOTHING;
INSERT INTO supplier_blocks(account_id,scope,model,kind,until_at,code,message,observed_at,auth_revision)
SELECT h.account_id,'account','','quota',h.cooldown_until,h.cooldown_code,NULL,
       COALESCE(h.cooldown_observed_at,floor(extract(epoch FROM clock_timestamp()))::bigint),a.auth_revision
FROM supplier_health h JOIN supplier_accounts a ON a.id=h.account_id
WHERE h.cooldown_kind='quota_exhausted' AND h.cooldown_auth_revision=a.auth_revision
  AND h.cooldown_until>floor(extract(epoch FROM clock_timestamp()))::bigint
ON CONFLICT(account_id,scope,model) DO NOTHING;
INSERT INTO supplier_blocks(account_id,scope,model,kind,until_at,code,message,observed_at,auth_revision)
SELECT c.account_id,'model',c.model,'quota',c.cooldown_until,c.cooldown_code,NULL,c.cooldown_observed_at,a.auth_revision
FROM supplier_model_cooldowns c JOIN supplier_accounts a ON a.id=c.account_id
WHERE c.cooldown_auth_revision=a.auth_revision
  AND c.cooldown_until>floor(extract(epoch FROM clock_timestamp()))::bigint;

DROP TABLE supplier_model_cooldowns;
ALTER TABLE supplier_health
    DROP COLUMN rejected_auth_revision,
    DROP COLUMN cooldown_kind,
    DROP COLUMN cooldown_until,
    DROP COLUMN cooldown_auth_revision,
    DROP COLUMN cooldown_code,
    DROP COLUMN cooldown_observed_at,
    DROP COLUMN payment_required_at,
    DROP COLUMN payment_required_code;
-- The observation fence: every block write advances it, so an availability read that
-- began before a newer failure cannot clear that failure.
ALTER TABLE supplier_health RENAME COLUMN cooldown_revision TO observation_revision;

-- A refresh was a retry on the same supplier. Keep the recorded attempt and
-- measurements while adopting the current recovery vocabulary atomically.
LOCK TABLE usage_attempts IN ACCESS EXCLUSIVE MODE;
ALTER TABLE usage_attempts DROP CONSTRAINT usage_attempts_recovery_check;
ALTER TABLE usage_attempts DISABLE TRIGGER usage_attempt_immutable;
UPDATE usage_attempts SET recovery='retry' WHERE recovery='refresh';
ALTER TABLE usage_attempts ENABLE TRIGGER usage_attempt_immutable;
ALTER TABLE usage_attempts ADD CONSTRAINT usage_attempts_recovery_check
    CHECK(recovery IN ('retry','failover'));

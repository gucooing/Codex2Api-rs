-- Request throttling belongs to client retry policy, not supplier availability.
UPDATE supplier_health
SET cooldown_kind=NULL,cooldown_until=NULL,cooldown_auth_revision=NULL,
    cooldown_code=NULL,cooldown_observed_at=NULL
WHERE cooldown_kind='rate_limited';

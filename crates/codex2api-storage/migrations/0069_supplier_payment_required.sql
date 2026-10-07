-- Billing rejection is independent of credential validity and quota resets.
-- Only explicit reauthorization or an administrator reset clears this state.
ALTER TABLE supplier_health ADD COLUMN payment_required_at TEXT;
ALTER TABLE supplier_health ADD COLUMN payment_required_code TEXT;

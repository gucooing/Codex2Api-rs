-- Budget checks read indexed amounts and timestamps without loading request price JSON.
CREATE INDEX usage_records_budget_window
ON usage_records(subject_id, requested_at_ms, quota_reset_credit_id, cost_nano_usd);

CREATE INDEX usage_records_billing_totals
ON usage_records(subject_id, cost_nano_usd, billing_status);

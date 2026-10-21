CREATE INDEX usage_records_supplier_platform_time
    ON usage_records(account_provider_id, requested_at_ms DESC, id DESC);

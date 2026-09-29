-- Legacy transport diagnostics remain inspectable, but cannot disable credentials.
ALTER TABLE supplier_health ADD COLUMN rejected_auth_revision INTEGER;
UPDATE supplier_health SET rejected_auth_revision=(SELECT auth_revision FROM supplier_accounts WHERE id=account_id)
WHERE error_message='ChatGPT 官方通信失败（HTTP 401）';

-- Preserve the real HTTP handshake separately from the generation error.
-- No historical status/category is inferred.
ALTER TABLE usage_records ADD COLUMN failure_kind TEXT;
ALTER TABLE usage_records ADD COLUMN failure_status INTEGER;

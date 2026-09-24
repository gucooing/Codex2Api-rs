-- Retire client API keys without erasing their historical accounting records.
ALTER TABLE usage_records RENAME COLUMN api_key_id TO subject_id;
ALTER TABLE usage_records RENAME COLUMN api_key_name TO subject_name;
ALTER TABLE usage_records ADD COLUMN subject_kind TEXT NOT NULL DEFAULT 'virtual_account'
    CHECK(subject_kind IN ('virtual_account','retired_api_key'));
UPDATE usage_records SET subject_kind='retired_api_key'
WHERE NOT EXISTS(SELECT 1 FROM virtual_accounts v WHERE v.id=usage_records.subject_id);
DROP TABLE proxy_api_keys;
DROP INDEX IF EXISTS usage_records_key_time;
CREATE INDEX usage_records_subject_time ON usage_records(subject_kind,subject_id,requested_at_ms DESC);

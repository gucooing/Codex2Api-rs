ALTER TABLE usage_records ADD COLUMN error_message TEXT;

-- Only repair historical outcomes that the stored HTTP status proves incorrect.
UPDATE usage_records
SET status = 'failed',
    error_message = 'HTTP ' || http_status || '：历史记录未保存具体错误内容。'
WHERE status = 'completed' AND http_status >= 400;

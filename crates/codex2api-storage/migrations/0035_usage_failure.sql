-- Store only bounded error metadata, never entire prompts or response bodies.
ALTER TABLE usage_records ADD COLUMN error_code TEXT;
ALTER TABLE usage_records ADD COLUMN error_message TEXT;

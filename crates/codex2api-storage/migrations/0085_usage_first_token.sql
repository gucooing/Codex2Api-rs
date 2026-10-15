-- First-event latency cannot be converted into first-token latency.
ALTER TABLE usage_records DROP COLUMN first_byte_ms;
ALTER TABLE usage_records ADD COLUMN first_token_ms INTEGER;

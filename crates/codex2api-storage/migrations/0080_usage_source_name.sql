ALTER TABLE usage_records ADD COLUMN source_name TEXT;

CREATE TRIGGER usage_source_name_immutable
BEFORE UPDATE OF source_name ON usage_records
WHEN NEW.source_name IS NOT OLD.source_name
BEGIN SELECT RAISE(ABORT, 'usage source name is immutable'); END;

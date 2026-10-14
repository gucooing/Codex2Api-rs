-- The caller's response mode is independent of the supplier's HTTP framing.
-- Historical HTTP records did not capture it and remain unknown.
ALTER TABLE usage_records ADD COLUMN stream INTEGER CHECK(stream IS NULL OR stream IN (0,1));
CREATE TRIGGER usage_stream_immutable BEFORE UPDATE OF stream ON usage_records
WHEN NEW.stream IS NOT OLD.stream
BEGIN SELECT RAISE(ABORT,'usage response mode is immutable'); END;

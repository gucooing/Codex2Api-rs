-- Missing historical upstream paths remain unknown.
ALTER TABLE usage_records ADD COLUMN upstream_endpoint TEXT;

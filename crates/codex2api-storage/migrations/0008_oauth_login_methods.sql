ALTER TABLE oauth_pending ADD COLUMN flow_data_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE oauth_pending ADD COLUMN last_polled_at_ms INTEGER;

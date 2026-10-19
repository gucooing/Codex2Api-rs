-- Caller addresses of historical requests were not observed and remain unknown.
ALTER TABLE usage_records ADD COLUMN client_ip TEXT CHECK(client_ip IS NULL OR length(client_ip) BETWEEN 2 AND 45);

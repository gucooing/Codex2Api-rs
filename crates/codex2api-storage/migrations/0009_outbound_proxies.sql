CREATE TABLE outbound_proxies (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    created_at TEXT NOT NULL
);
ALTER TABLE accounts ADD COLUMN proxy_id TEXT REFERENCES outbound_proxies(id) ON DELETE RESTRICT;

-- Per-account official Codex CLI HTTP fingerprint (application-layer).
-- Frozen at account creation; reused for every upstream request.

-- Existing rows get an empty object only as a migration placeholder.
-- New accounts always write a full official-CLI fingerprint JSON; never leave this empty.
ALTER TABLE accounts ADD COLUMN http_fingerprint_json TEXT NOT NULL DEFAULT '{}';

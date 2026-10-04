-- Model availability is discovered through authenticated Grok Build /models.
-- The client's offline defaults are not an entitlement or the service's catalog.
CREATE TABLE grok_model_observations (
    account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    descriptor_json TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    auth_revision INTEGER NOT NULL,
    PRIMARY KEY(account_id,model)
);
CREATE TABLE grok_catalog_snapshots (
    account_id TEXT PRIMARY KEY REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    response_json TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    auth_revision INTEGER NOT NULL
);
-- Preserve any operator decision or price. Unreviewed bootstrap models await discovery.
UPDATE model_catalog SET enabled=0 WHERE provider_id='grok' AND revision=1
    AND NOT EXISTS(SELECT 1 FROM model_prices p WHERE p.provider_id='grok' AND p.model=model_catalog.model);
INSERT OR IGNORE INTO meta(key,value)
SELECT 'model_preset_pending:grok:'||model,'1' FROM model_catalog
WHERE provider_id='grok' AND enabled=0 AND revision=1;

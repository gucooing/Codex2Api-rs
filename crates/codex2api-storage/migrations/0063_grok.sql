-- A distinct platform, with no inferred prices or inherited supplier data.
INSERT INTO providers(id,name) VALUES('grok','Grok');
INSERT INTO virtual_plans(id,provider_id,name,plan_type,config,allow_purchase)
VALUES('platform-free-grok','grok','Grok Free','free',
    '{"model_access":"none","models":[],"spending_windows":[{"duration_seconds":2592000,"cost_limit_usd":"0"}],"duration_days":30}',0);
INSERT INTO platform_free_plans(provider_id,plan_id) VALUES('grok','platform-free-grok');
INSERT INTO model_catalog(provider_id,model,kind,enabled) VALUES
    ('grok','grok-4.6','text',1),('grok','grok-4.5','text',1);

-- Existing users receive a new, independent Free identity. Standalone accounts stay intact.
CREATE TEMP TABLE grok_new_users AS SELECT id AS user_id,lower(hex(randomblob(16))) AS virtual_id FROM users;
INSERT INTO virtual_accounts(id,provider_id,username,password_hash,name,email,plan_id,plan_type,enabled,created_at,subscription_started_at)
SELECT virtual_id,'grok','user-'||virtual_id,'','','','platform-free-grok','free',1,
    strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now') FROM grok_new_users;
INSERT INTO user_subscriptions(user_id,provider_id,virtual_account_id)
SELECT user_id,'grok',virtual_id FROM grok_new_users;
INSERT INTO execution_routes(virtual_account_id,provider_id)
SELECT virtual_id,'grok' FROM grok_new_users;
DROP TABLE grok_new_users;

ALTER TABLE virtual_authorization_codes ADD COLUMN oidc_nonce TEXT;
ALTER TABLE virtual_device_authorizations ADD COLUMN provider_id TEXT NOT NULL DEFAULT 'chatgpt' REFERENCES providers(id);
ALTER TABLE virtual_device_authorizations ADD COLUMN poll_interval INTEGER NOT NULL DEFAULT 5;

-- Register the Antigravity supplier platform and model-scoped quota cooldowns.
-- An exhausted model keeps the account eligible for its other models.
INSERT INTO providers (id,name) VALUES ('antigravity','Antigravity') ON CONFLICT (id) DO NOTHING;
CREATE TABLE supplier_model_cooldowns (
    account_id TEXT NOT NULL REFERENCES supplier_accounts(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    cooldown_until BIGINT NOT NULL,
    cooldown_auth_revision BIGINT NOT NULL,
    cooldown_code TEXT,
    cooldown_observed_at BIGINT NOT NULL,
    PRIMARY KEY(account_id,model)
);

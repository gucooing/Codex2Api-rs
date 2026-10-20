-- The API-key supplier channel: accounts hold a Base URL, a key and a wire format in
-- their credentials and carry no device fingerprint. The frozen identity inputs of
-- the official-client channels stay required for those channels only.
INSERT INTO providers (id,name) VALUES ('api','API') ON CONFLICT (id) DO NOTHING;

ALTER TABLE supplier_accounts
    ALTER COLUMN os_type DROP NOT NULL,
    ALTER COLUMN os_version DROP NOT NULL,
    ALTER COLUMN arch DROP NOT NULL,
    ADD CONSTRAINT supplier_account_identity_inputs CHECK (
        (provider_id = 'api') = (os_type IS NULL AND os_version IS NULL AND arch IS NULL)
    );

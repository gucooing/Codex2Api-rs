CREATE TABLE operation_prices (
    provider_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    price_nano_usd INTEGER CHECK (price_nano_usd >= 0),
    revision INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (provider_id, operation)
);

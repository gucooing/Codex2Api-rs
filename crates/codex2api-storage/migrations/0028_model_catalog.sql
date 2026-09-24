CREATE TABLE model_catalog (
    model TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('text','image')),
    enabled INTEGER NOT NULL DEFAULT 1,
    deleted INTEGER NOT NULL DEFAULT 0,
    revision INTEGER NOT NULL DEFAULT 1
);
INSERT INTO model_catalog(model,kind) SELECT DISTINCT model,'text' FROM model_prices;
INSERT OR IGNORE INTO model_catalog(model,kind)
SELECT COALESCE(actual_model,model),CASE WHEN MAX(endpoint LIKE '%/images/%') THEN 'image' ELSE 'text' END
FROM usage_records WHERE COALESCE(actual_model,model,'')!='' GROUP BY COALESCE(actual_model,model);

CREATE TABLE model_image_prices (
    model TEXT NOT NULL REFERENCES model_catalog(model),
    resolution TEXT NOT NULL,
    price_nano_usd INTEGER NOT NULL CHECK(price_nano_usd>=0),
    PRIMARY KEY(model,resolution)
);
ALTER TABLE usage_records ADD COLUMN image_count INTEGER;
ALTER TABLE usage_records ADD COLUMN image_usage_json TEXT;

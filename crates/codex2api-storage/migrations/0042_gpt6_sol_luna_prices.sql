-- Official prices fetched 2026-09-25:
-- https://developers.openai.com/api/docs/pricing
-- https://developers.openai.com/api/docs/models/gpt-6-sol
-- https://developers.openai.com/api/docs/models/gpt-6-luna
-- Micro-USD / million tokens. Above 272,000 input tokens, input/cache
-- rates are doubled and output is 1.5x for the entire request.
-- Seed only absent or untouched unpriced catalog entries. Never fill missing
-- tiers in an administrator's custom price set or revive deleted models.
CREATE TEMP TABLE gpt6_price_seed (model TEXT PRIMARY KEY);
INSERT INTO gpt6_price_seed
SELECT value FROM json_each('["gpt-6-sol","gpt-6-luna"]') AS names
WHERE NOT EXISTS(SELECT 1 FROM model_prices p WHERE p.provider_id='chatgpt' AND p.model=names.value)
  AND NOT EXISTS(SELECT 1 FROM model_catalog c WHERE c.provider_id='chatgpt' AND c.model=names.value
                 AND (c.deleted=1 OR c.kind!='text' OR c.revision>1));

INSERT OR IGNORE INTO model_catalog(provider_id,model,kind)
SELECT 'chatgpt',model,'text' FROM gpt6_price_seed;

WITH base(model,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate) AS (
    VALUES ('gpt-6-sol',0,2000000,200000,2500000,10000000),
           ('gpt-6-sol',272001,4000000,400000,5000000,15000000),
           ('gpt-6-luna',0,100000,10000,125000,500000),
           ('gpt-6-luna',272001,200000,20000,250000,750000)
), tiers(tier,numerator,denominator) AS (
    VALUES ('standard',1,1),('fast',2,1),('flex',1,2)
)
INSERT INTO model_prices(provider_id,model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source,revision)
SELECT 'chatgpt',base.model,tiers.tier,base.min_input_tokens,
       base.input_rate*numerator/denominator,base.cached_rate*numerator/denominator,
       base.cache_write_rate*numerator/denominator,base.output_rate*numerator/denominator,
       'official:2026-09-25',1
FROM base JOIN gpt6_price_seed ON gpt6_price_seed.model=base.model CROSS JOIN tiers;
DROP TABLE gpt6_price_seed;

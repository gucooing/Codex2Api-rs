-- Official text-token prices verified 2026-09-20:
-- https://developers.openai.com/api/docs/pricing
-- Long context: >272,000 input tokens; applies to the entire request.
CREATE TABLE model_prices (
 model TEXT NOT NULL, tier TEXT NOT NULL, min_input_tokens INTEGER NOT NULL DEFAULT 0,
 input_rate INTEGER NOT NULL CHECK(input_rate>=0), cached_rate INTEGER NOT NULL CHECK(cached_rate>=0),
 cache_write_rate INTEGER NOT NULL CHECK(cache_write_rate>=0), output_rate INTEGER NOT NULL CHECK(output_rate>=0),
 source TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 1,
 PRIMARY KEY(model,tier,min_input_tokens)
);
-- Rates are integer micro-USD per million tokens. Costs are nano-USD.
ALTER TABLE usage_records ADD COLUMN pricing_snapshot_json TEXT;
ALTER TABLE usage_records ADD COLUMN cost_nano_usd INTEGER;
ALTER TABLE usage_records ADD COLUMN billing_status TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE usage_records ADD COLUMN billing_model TEXT;
-- Preserve old settings for audit; Token limits cannot be converted to dollars.
UPDATE virtual_client_state SET state_key='legacy_token_quota' WHERE state_key='quota';
INSERT INTO meta(key,value) VALUES('billing_started_at',strftime('%Y-%m-%dT%H:%M:%SZ','now'));
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-6-astra','standard',0,10000000,1000000,12500000,50000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-6-astra','standard',272001,20000000,2000000,25000000,75000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-6-astra','fast',0,20000000,2000000,25000000,100000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-6-astra','fast',272001,40000000,4000000,50000000,150000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-6-astra','flex',0,5000000,500000,6250000,25000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-6-astra','flex',272001,10000000,1000000,12500000,37500000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-sol','standard',0,4000000,400000,5000000,20000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-sol','standard',272001,8000000,800000,10000000,30000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-sol','fast',0,8000000,800000,10000000,40000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-sol','fast',272001,16000000,1600000,20000000,60000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-sol','flex',0,2000000,200000,2500000,10000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-sol','flex',272001,4000000,400000,5000000,15000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6','standard',0,4000000,400000,5000000,20000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6','standard',272001,8000000,800000,10000000,30000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6','fast',0,8000000,800000,10000000,40000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6','fast',272001,16000000,1600000,20000000,60000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6','flex',0,2000000,200000,2500000,10000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6','flex',272001,4000000,400000,5000000,15000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-terra','standard',0,2000000,200000,2500000,12000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-terra','standard',272001,4000000,400000,5000000,18000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-terra','fast',0,4000000,400000,5000000,24000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-terra','fast',272001,8000000,800000,10000000,36000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-terra','flex',0,1000000,100000,1250000,6000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-terra','flex',272001,2000000,200000,2500000,9000000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-luna','standard',0,200000,20000,250000,1200000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-luna','standard',272001,400000,40000,500000,1800000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-luna','fast',0,400000,40000,500000,2400000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-luna','fast',272001,800000,80000,1000000,3600000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-luna','flex',0,100000,10000,125000,600000,'official:2026-09-20');
INSERT INTO model_prices(model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source) VALUES('gpt-5.6-luna','flex',272001,200000,20000,250000,900000,'official:2026-09-20');

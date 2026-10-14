-- Correct only the unchanged generated GPT-5.5 default. Administrator custom
-- prices, enablement and immutable request snapshots are not rewritten.
CREATE TEMP TABLE global_fast_default AS
SELECT c.provider_id,c.model
FROM model_catalog c
JOIN model_prices f ON f.provider_id=c.provider_id AND f.model=c.model
    AND f.tier='fast' AND f.min_input_tokens=0
JOIN model_prices s ON s.provider_id=c.provider_id AND s.model=c.model
    AND s.tier='standard' AND s.min_input_tokens=0
JOIN model_prices l ON l.provider_id=c.provider_id AND l.model=c.model
    AND l.tier='standard' AND l.min_input_tokens=272001
WHERE c.provider_id='chatgpt' AND c.model='gpt-5.5' AND c.kind='text' AND c.deleted=0
  AND f.source LIKE 'preset:%' AND f.max_input_tokens=272000
  AND f.input_rate=12500000 AND f.cached_rate=1250000
  AND f.cache_write_rate=12500000 AND f.output_rate=75000000
  AND s.source LIKE 'preset:%' AND (s.max_input_tokens IS NULL OR s.max_input_tokens=272000)
  AND s.input_rate=5000000 AND s.cached_rate=500000
  AND s.cache_write_rate=5000000 AND s.output_rate=30000000
  AND l.source LIKE 'preset:%' AND l.max_input_tokens IS NULL
  AND l.input_rate=10000000 AND l.cached_rate=1000000
  AND l.cache_write_rate=10000000 AND l.output_rate=45000000
  AND (SELECT COUNT(*) FROM model_prices p WHERE p.provider_id=c.provider_id AND p.model=c.model AND p.tier='standard')=2
  AND (SELECT COUNT(*) FROM model_prices p WHERE p.provider_id=c.provider_id AND p.model=c.model AND p.tier='fast')=1;

INSERT INTO model_prices(provider_id,model,tier,min_input_tokens,max_input_tokens,
    input_rate,cached_rate,cache_write_rate,output_rate,source,revision)
SELECT provider_id,model,'fast',272001,NULL,25000000,2500000,25000000,112500000,'preset:2026-10-14',1
FROM global_fast_default;

UPDATE model_prices SET max_input_tokens=NULL,source='preset:2026-10-14',revision=revision+1
WHERE tier='fast' AND min_input_tokens=0
  AND (provider_id,model) IN (SELECT provider_id,model FROM global_fast_default);
UPDATE model_catalog SET revision=revision+1
WHERE (provider_id,model) IN (SELECT provider_id,model FROM global_fast_default);
DROP TABLE global_fast_default;

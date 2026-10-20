-- Catalog platforms label a model's vendor independently of the supplier channels
-- that serve it. The labels that reused channel names become their vendors; "other"
-- stays for vendors without a label of their own.
UPDATE model_catalog SET platform = CASE platform
        WHEN 'chatgpt' THEN 'openai'
        WHEN 'grok' THEN 'xai'
        WHEN 'gemini' THEN 'google'
        WHEN 'claude' THEN 'anthropic'
    END
WHERE platform IN ('chatgpt','grok','gemini','claude');

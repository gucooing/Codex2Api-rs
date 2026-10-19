-- Initial catalog and plans; no user subscriptions are created.

INSERT INTO providers (id,name) VALUES
('chatgpt','ChatGPT'),
('grok','Grok'),
('antigravity','Antigravity');

INSERT INTO model_catalog (provider_id,model,kind,enabled,deleted,revision) VALUES
('chatgpt','gpt-5.6','text',TRUE,FALSE,1),
('chatgpt','gpt-5.6-luna','text',TRUE,FALSE,1),
('chatgpt','gpt-5.6-sol','text',TRUE,FALSE,1),
('chatgpt','gpt-5.6-terra','text',TRUE,FALSE,1),
('chatgpt','gpt-6-astra','text',TRUE,FALSE,1),
('chatgpt','gpt-6-sol','text',TRUE,FALSE,1),
('chatgpt','gpt-6-luna','text',TRUE,FALSE,1),
('grok','grok-4.6','text',FALSE,FALSE,1),
('grok','grok-4.5','text',FALSE,FALSE,1);

INSERT INTO model_prices (provider_id,model,tier,min_input_tokens,input_rate,cached_rate,cache_write_rate,output_rate,source,revision,max_input_tokens) VALUES
('chatgpt','gpt-6-astra','standard',0,10000000,1000000,12500000,50000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-6-astra','standard',272001,20000000,2000000,25000000,75000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-6-astra','fast',0,20000000,2000000,25000000,100000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-6-astra','fast',272001,40000000,4000000,50000000,150000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-6-astra','flex',0,5000000,500000,6250000,25000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-6-astra','flex',272001,10000000,1000000,12500000,37500000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-sol','standard',0,4000000,400000,5000000,20000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-sol','standard',272001,8000000,800000,10000000,30000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-sol','fast',0,8000000,800000,10000000,40000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-sol','fast',272001,16000000,1600000,20000000,60000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-sol','flex',0,2000000,200000,2500000,10000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-sol','flex',272001,4000000,400000,5000000,15000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6','standard',0,4000000,400000,5000000,20000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6','standard',272001,8000000,800000,10000000,30000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6','fast',0,8000000,800000,10000000,40000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6','fast',272001,16000000,1600000,20000000,60000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6','flex',0,2000000,200000,2500000,10000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6','flex',272001,4000000,400000,5000000,15000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-terra','standard',0,2000000,200000,2500000,12000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-terra','standard',272001,4000000,400000,5000000,18000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-terra','fast',0,4000000,400000,5000000,24000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-terra','fast',272001,8000000,800000,10000000,36000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-terra','flex',0,1000000,100000,1250000,6000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-terra','flex',272001,2000000,200000,2500000,9000000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-luna','standard',0,200000,20000,250000,1200000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-luna','standard',272001,400000,40000,500000,1800000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-luna','fast',0,400000,40000,500000,2400000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-luna','fast',272001,800000,80000,1000000,3600000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-luna','flex',0,100000,10000,125000,600000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-5.6-luna','flex',272001,200000,20000,250000,900000,'official:2026-09-20',1,NULL),
('chatgpt','gpt-6-sol','standard',0,2000000,200000,2500000,10000000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-sol','fast',0,4000000,400000,5000000,20000000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-sol','flex',0,1000000,100000,1250000,5000000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-sol','standard',272001,4000000,400000,5000000,15000000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-sol','fast',272001,8000000,800000,10000000,30000000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-sol','flex',272001,2000000,200000,2500000,7500000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-luna','standard',0,100000,10000,125000,500000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-luna','fast',0,200000,20000,250000,1000000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-luna','flex',0,50000,5000,62500,250000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-luna','standard',272001,200000,20000,250000,750000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-luna','fast',272001,400000,40000,500000,1500000,'official:2026-09-25',1,NULL),
('chatgpt','gpt-6-luna','flex',272001,100000,10000,125000,375000,'official:2026-09-25',1,NULL);

INSERT INTO virtual_plans (id,name,plan_type,config,allow_purchase,revision,updated_at_ms,provider_id) VALUES
('plus','ChatGPT Plus','plus','{"models":[],"model_access":"all","spending_windows":[],"rpm_limit":40,"concurrency_limit":10}',TRUE,0,0,'chatgpt'),
('pro','ChatGPT Pro 200','pro','{"models":[],"model_access":"all","spending_windows":[],"rpm_limit":40,"concurrency_limit":10}',TRUE,0,0,'chatgpt'),
('business','ChatGPT Business','business','{"models":[],"model_access":"all","spending_windows":[],"rpm_limit":40,"concurrency_limit":10}',TRUE,0,0,'chatgpt'),
('enterprise','ChatGPT Enterprise','enterprise','{"models":[],"model_access":"all","spending_windows":[],"rpm_limit":40,"concurrency_limit":10}',TRUE,0,0,'chatgpt'),
('edu','ChatGPT Edu','edu','{"models":[],"model_access":"all","spending_windows":[],"rpm_limit":40,"concurrency_limit":10}',TRUE,0,0,'chatgpt');

INSERT INTO meta(key,value) VALUES('billing_started_at',to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'));

INSERT INTO billing_cache_revisions(provider_id,revision) SELECT id,0 FROM providers ON CONFLICT DO NOTHING;

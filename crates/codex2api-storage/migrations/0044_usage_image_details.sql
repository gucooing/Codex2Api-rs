-- Keep image edit input dimensions and the resolved billing tier with each ledger row.
ALTER TABLE usage_records ADD COLUMN image_input_usage_json TEXT;
ALTER TABLE usage_records ADD COLUMN billing_tier TEXT;

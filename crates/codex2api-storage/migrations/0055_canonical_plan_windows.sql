-- Normalize stored plan windows once; runtime uses the canonical nested form.
UPDATE virtual_plans SET config=json_set(config,'$.spending_windows',json(
    CASE WHEN json_extract(config,'$.primary_cost_limit_usd') IS NULL
          AND json_extract(config,'$.weekly_cost_limit_usd') IS NULL THEN '[]'
    WHEN json_extract(config,'$.primary_cost_limit_usd') IS NULL THEN json_array(
        json_object('duration_seconds',604800,'cost_limit_usd',CAST(json_extract(config,'$.weekly_cost_limit_usd') AS TEXT)))
    ELSE json_array(
        json_object('duration_seconds',604800,'cost_limit_usd',CAST(json_extract(config,'$.weekly_cost_limit_usd') AS TEXT)),
        json_object('duration_seconds',18000,'cost_limit_usd',CAST(json_extract(config,'$.primary_cost_limit_usd') AS TEXT))) END))
WHERE json_type(config,'$.primary_cost_limit_usd') IS NOT NULL OR json_type(config,'$.weekly_cost_limit_usd') IS NOT NULL;
UPDATE virtual_plans SET config=json_remove(config,'$.primary_cost_limit_usd','$.weekly_cost_limit_usd');

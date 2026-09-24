-- Restore independently configurable spending windows without changing existing budgets.
UPDATE virtual_client_state
SET value_json=json_set(value_json,
    '$.primary_cost_limit_usd', json_extract(value_json,'$.primary_cost_limit_usd'),
    '$.weekly_cost_limit_usd', json_extract(value_json,'$.weekly_cost_limit_usd')),
    revision=revision+1
WHERE state_key='quota'
  AND (json_type(value_json,'$.primary_cost_limit_usd') IS NULL
       OR json_type(value_json,'$.weekly_cost_limit_usd') IS NULL);

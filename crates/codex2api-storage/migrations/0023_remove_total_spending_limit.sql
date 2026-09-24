-- Only the 5-hour and weekly spending windows remain configurable/enforced.
UPDATE virtual_client_state
SET value_json=json_remove(value_json,'$.total_cost_limit_usd'), revision=revision+1
WHERE state_key='quota' AND json_type(value_json,'$.total_cost_limit_usd') IS NOT NULL;

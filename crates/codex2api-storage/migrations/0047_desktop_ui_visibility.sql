-- These are service-owned visibility controls, not client preferences. Preserve
-- explicit values and the existing tab policy when extending persisted records.
UPDATE virtual_client_state
SET value_json = json_insert(value_json,
        '$.navigation_rail_enabled', json('true'),
        '$.unified_composer_enabled', json('true'),
        '$.reset_credits_visible', json('true')),
    revision = revision + 1
WHERE state_key = 'desktop_ui_policy'
  AND (json_type(value_json, '$.navigation_rail_enabled') IS NULL
    OR json_type(value_json, '$.unified_composer_enabled') IS NULL
    OR json_type(value_json, '$.reset_credits_visible') IS NULL);

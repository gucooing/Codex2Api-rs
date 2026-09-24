ALTER TABLE virtual_accounts ADD COLUMN subscription_started_at TEXT;
UPDATE virtual_accounts SET subscription_started_at = COALESCE((
    SELECT strftime('%Y-%m-%dT%H:%M:%fZ', created_at_ms / 1000.0, 'unixepoch')
    FROM virtual_resources
    WHERE virtual_account_id = virtual_accounts.id AND kind = 'subscription_operation'
      AND json_extract(value_json, '$.plan_id') = virtual_accounts.plan_id
      AND (json_extract(value_json, '$.operation') IN ('grant', 'change_plan')
        OR (json_extract(value_json, '$.operation') = 'renew'
          AND unixepoch(json_extract(value_json, '$.previous_expires_at')) <= created_at_ms / 1000))
    ORDER BY created_at_ms DESC LIMIT 1
), created_at);

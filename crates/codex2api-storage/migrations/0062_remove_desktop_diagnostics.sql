DROP TABLE desktop_diagnostics;

UPDATE meta
SET value = json_remove(value, '$.collect_diagnostics')
WHERE key = 'desktop_support';

DELETE FROM oauth_missing_endpoints WHERE path IN (
    '/api/oauth/chatgpt/ces/v1/telemetry/intake',
    '/api/oauth/chatgpt/ces/v1/rgstr',
    '/api/oauth/chatgpt/ces/statsc/flush',
    '/api/oauth/chatgpt/v1/sdk_exception'
);

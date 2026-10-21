-- Per-model quota snapshots cannot provide the desktop's grouped quota windows.
-- Invalidate only this derived cache; the next explicit official refresh fills it.
DELETE FROM account_quota_cache q USING supplier_accounts a
WHERE q.account_id=a.id AND a.provider_id='antigravity';

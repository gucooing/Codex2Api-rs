-- Plan publishing is independent of administrative assignment and active benefits.
ALTER TABLE virtual_plans RENAME COLUMN enabled TO allow_purchase;
UPDATE virtual_plans SET allow_purchase=0 WHERE plan_type='free';

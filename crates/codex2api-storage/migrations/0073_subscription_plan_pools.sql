-- Ordinary subscriptions resolve supply from their effective plan. Only standalone
-- virtual accounts retain an independently configured tag on execution_routes.
CREATE VIEW regular_subscription_pools AS
SELECT v.id AS virtual_account_id,v.provider_id,p.id AS plan_id,
       json_extract(p.config,'$.supplier_tag_id') AS tag_id
FROM regular_platforms v
JOIN platform_free_plans f ON f.provider_id=v.provider_id
JOIN virtual_plans p ON p.id=CASE
    WHEN v.plan_type!='free' AND v.subscription_expires_at IS NOT NULL
         AND unixepoch(v.subscription_expires_at)<=unixepoch() THEN f.plan_id
    ELSE v.plan_id END;

DROP TRIGGER execution_route_pool_insert;
DROP TRIGGER execution_route_pool_update;
DROP TRIGGER supplier_tag_member_removed;

UPDATE execution_routes SET tag_id=NULL,supplier_account_id=NULL,revision=revision+1
WHERE virtual_account_id IN(SELECT id FROM regular_platforms);
INSERT INTO execution_routes(virtual_account_id,provider_id)
SELECT id,provider_id FROM regular_platforms WHERE 1
ON CONFLICT(virtual_account_id,provider_id) DO NOTHING;

CREATE VIEW effective_execution_routes AS
SELECT r.virtual_account_id,r.provider_id,r.supplier_account_id,r.revision,
       CASE WHEN p.virtual_account_id IS NOT NULL THEN p.tag_id ELSE r.tag_id END AS tag_id
FROM execution_routes r
LEFT JOIN regular_subscription_pools p ON p.virtual_account_id=r.virtual_account_id;

CREATE TRIGGER execution_route_pool_insert BEFORE INSERT ON execution_routes
WHEN (NEW.tag_id IS NOT NULL AND (
        EXISTS(SELECT 1 FROM regular_platforms WHERE id=NEW.virtual_account_id)
        OR NOT EXISTS(SELECT 1 FROM supplier_tags WHERE id=NEW.tag_id AND provider_id=NEW.provider_id)))
 OR (NEW.supplier_account_id IS NOT NULL AND NOT EXISTS(
        SELECT 1 FROM supplier_tag_members m WHERE m.account_id=NEW.supplier_account_id
        AND m.tag_id=COALESCE((SELECT tag_id FROM regular_subscription_pools
                              WHERE virtual_account_id=NEW.virtual_account_id),NEW.tag_id)))
BEGIN SELECT RAISE(ABORT,'execution route pool mismatch'); END;
CREATE TRIGGER execution_route_pool_update BEFORE UPDATE OF tag_id,supplier_account_id ON execution_routes
WHEN (NEW.tag_id IS NOT NULL AND (
        EXISTS(SELECT 1 FROM regular_platforms WHERE id=NEW.virtual_account_id)
        OR NOT EXISTS(SELECT 1 FROM supplier_tags WHERE id=NEW.tag_id AND provider_id=NEW.provider_id)))
 OR (NEW.supplier_account_id IS NOT NULL AND NOT EXISTS(
        SELECT 1 FROM supplier_tag_members m WHERE m.account_id=NEW.supplier_account_id
        AND m.tag_id=COALESCE((SELECT tag_id FROM regular_subscription_pools
                              WHERE virtual_account_id=NEW.virtual_account_id),NEW.tag_id)))
BEGIN SELECT RAISE(ABORT,'execution route pool mismatch'); END;

CREATE TRIGGER supplier_tag_member_removed AFTER DELETE ON supplier_tag_members
BEGIN
    UPDATE execution_routes SET supplier_account_id=NULL,revision=revision+1
    WHERE supplier_account_id=OLD.account_id AND virtual_account_id IN(
        SELECT virtual_account_id FROM effective_execution_routes WHERE tag_id=OLD.tag_id
    );
END;
CREATE TRIGGER plan_supplier_pool_changed AFTER UPDATE OF config ON virtual_plans
WHEN json_extract(OLD.config,'$.supplier_tag_id') IS NOT json_extract(NEW.config,'$.supplier_tag_id')
BEGIN
    UPDATE execution_routes SET supplier_account_id=NULL,revision=revision+1
    WHERE virtual_account_id IN(SELECT virtual_account_id FROM regular_subscription_pools WHERE plan_id=NEW.id);
END;
CREATE TRIGGER subscription_supplier_pool_changed
AFTER UPDATE OF plan_id,plan_type,subscription_expires_at ON platform_accounts
WHEN NEW.plan_id IS NOT OLD.plan_id OR NEW.plan_type IS NOT OLD.plan_type
     OR (NEW.subscription_expires_at IS NOT NULL AND unixepoch(NEW.subscription_expires_at)<=unixepoch())
        IS NOT (OLD.subscription_expires_at IS NOT NULL AND unixepoch(OLD.subscription_expires_at)<=unixepoch())
BEGIN
    UPDATE execution_routes SET supplier_account_id=NULL,revision=revision+1
    WHERE virtual_account_id=NEW.id AND virtual_account_id IN(SELECT id FROM regular_platforms);
END;

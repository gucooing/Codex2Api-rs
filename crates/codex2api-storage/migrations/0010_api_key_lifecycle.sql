-- An API key is enabled or disabled while it exists, may move to another available API
-- group, rotates to a new secret under the same identity and usage history, and is
-- deleted irreversibly: a deleted key leaves every list and stops authenticating while
-- its usage records keep their owner and name. Revocation is not a visible state;
-- keys that were revoked are deleted.
ALTER TABLE api_keys ADD COLUMN deleted BOOLEAN NOT NULL DEFAULT FALSE CHECK(deleted IN (FALSE,TRUE));

CREATE OR REPLACE FUNCTION api_key_identity_immutable_fn() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
IF NEW.id!=OLD.id OR NEW.user_id!=OLD.user_id OR NEW.created_at!=OLD.created_at
 OR (NEW.deleted AND NEW.enabled)
 OR (OLD.deleted AND (NOT NEW.deleted OR NEW.name!=OLD.name OR NEW.group_id!=OLD.group_id
     OR NEW.token_hash!=OLD.token_hash OR NEW.key_prefix!=OLD.key_prefix))
 OR (NEW.group_id!=OLD.group_id AND NOT EXISTS(SELECT 1 FROM api_groups WHERE id=NEW.group_id AND enabled=TRUE AND deleted=FALSE)) THEN
RAISE EXCEPTION USING MESSAGE = 'API key identity and deletion are immutable and a key moves only to an available API group', ERRCODE = '23514';
END IF;
RETURN NEW;
END;
$$;

UPDATE api_keys SET deleted=TRUE,enabled=FALSE WHERE revoked_at IS NOT NULL;
ALTER TABLE api_keys DROP COLUMN revoked_at;

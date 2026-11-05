//! Opaque channel state belonging to one consumer's exact supplier response.
use crate::{Result, RoutingKey, Storage};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

fn pin_key(owner: &RoutingKey, supplier: &str, revision: i64) -> String {
    format!(
        "chatgpt:response-pin:{}:{revision}:{:x}",
        owner.0,
        Sha256::digest(supplier.as_bytes()),
    )
}

static SAVE: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        "local owner=redis.call('GET',KEYS[1])
         if owner and owner~=ARGV[1] then return 0 end
         redis.call('SET',KEYS[1],ARGV[1],'EX',ARGV[3])
         redis.call('SET',KEYS[2],ARGV[2],'NX','EX',ARGV[3])
         return 1",
    )
});

impl Storage {
    /// Publish ownership before the response reaches the client, including in-flight reviews.
    pub async fn remember_chatgpt_response_pin(
        &self,
        owner: &RoutingKey,
        supplier: &str,
        revision: i64,
        state: &str,
    ) -> Result<()> {
        self.runtime
            .redis
            .script::<i64>(
                &SAVE
                    .key(self.runtime.redis.key(&owner.0))
                    .key(self.runtime.redis.key(&pin_key(owner, supplier, revision)))
                    .arg(supplier)
                    .arg(state)
                    .arg(self.gateway_settings().await?.sticky_ttl_seconds),
            )
            .await?;
        Ok(())
    }

    pub async fn chatgpt_response_pin(
        &self,
        owner: &RoutingKey,
        supplier: &str,
        revision: i64,
    ) -> Result<Option<String>> {
        // Ownership and channel state are read together; another consumer, supplier or login
        // cannot reuse a pin even if it knows the external response ID.
        let values: (Option<String>, Option<String>) = self
            .runtime
            .redis
            .command(
                redis::cmd("MGET")
                    .arg(self.runtime.redis.key(&owner.0))
                    .arg(self.runtime.redis.key(&pin_key(owner, supplier, revision))),
            )
            .await?;
        Ok((values.0.as_deref() == Some(supplier))
            .then_some(values.1)
            .flatten())
    }
}

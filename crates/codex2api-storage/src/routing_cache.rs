//! Routing bindings have explicit ownership, bounded lifetimes and batched I/O.
use crate::{Result, Storage};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

#[derive(Clone, Debug)]
pub struct RoutingKey(String);
impl RoutingKey {
    pub fn session(owner: &str, provider: &str, pool: &str, session_hash: &str) -> Self {
        Self(format!(
            "routing:session:{provider}:{pool}:{owner}:{session_hash}"
        ))
    }
    pub fn response(owner: &str, provider: &str, response: &str) -> Self {
        Self(format!(
            "routing:response:{provider}:{owner}:{:x}",
            Sha256::digest(response.as_bytes())
        ))
    }
    pub fn turn(owner: &str, provider: &str, turn: &str) -> Self {
        Self(format!(
            "routing:turn:{provider}:{owner}:{:x}",
            Sha256::digest(turn.as_bytes())
        ))
    }
}

static REFRESH: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        "if redis.call('GET',KEYS[1]) == ARGV[1] then return redis.call('EXPIRE',KEYS[1],ARGV[2]) end return 0",
    )
});
static FORGET: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        "if redis.call('GET',KEYS[1]) == ARGV[1] then return redis.call('DEL',KEYS[1]) end return 0",
    )
});

impl Storage {
    pub async fn routing_affinities<const N: usize>(
        &self,
        keys: [Option<&RoutingKey>; N],
    ) -> Result<[Option<String>; N]> {
        let mut result = std::array::from_fn(|_| None);
        let present: Vec<_> = keys
            .iter()
            .enumerate()
            .filter_map(|(i, k)| k.map(|k| (i, k)))
            .collect();
        if present.is_empty() {
            return Ok(result);
        }
        let mut command = redis::cmd("MGET");
        for (_, key) in &present {
            command.arg(self.runtime.redis.key(&key.0));
        }
        let values: Vec<Option<String>> = self.runtime.redis.command(&command).await?;
        for ((index, _), value) in present.into_iter().zip(values) {
            result[index] = value;
        }
        Ok(result)
    }
    pub async fn routing_affinity(&self, key: &RoutingKey) -> Result<Option<String>> {
        let [value] = self.routing_affinities([Some(key)]).await?;
        Ok(value)
    }
    pub async fn save_routing_affinities(
        &self,
        bindings: &[(&RoutingKey, &str)],
        ttl: u64,
    ) -> Result<()> {
        if bindings.is_empty() {
            return Ok(());
        }
        let mut pipeline = redis::pipe();
        pipeline.atomic();
        for (key, owner) in bindings {
            pipeline
                .cmd("SET")
                .arg(self.runtime.redis.key(&key.0))
                .arg(owner)
                .arg("EX")
                .arg(ttl)
                .ignore();
        }
        self.runtime.redis.pipeline(&pipeline).await
    }
    pub async fn save_routing_affinity(
        &self,
        key: &RoutingKey,
        owner: &str,
        ttl: u64,
    ) -> Result<()> {
        self.save_routing_affinities(&[(key, owner)], ttl).await
    }
    pub async fn refresh_routing_affinity(
        &self,
        key: &RoutingKey,
        owner: &str,
        ttl: u64,
    ) -> Result<()> {
        self.runtime
            .redis
            .script::<i64>(
                &REFRESH
                    .key(self.runtime.redis.key(&key.0))
                    .arg(owner)
                    .arg(ttl),
            )
            .await?;
        Ok(())
    }
    pub async fn forget_routing_affinity(&self, key: &RoutingKey, owner: &str) -> Result<()> {
        self.runtime
            .redis
            .script::<i64>(&FORGET.key(self.runtime.redis.key(&key.0)).arg(owner))
            .await?;
        Ok(())
    }
    pub async fn remember_response_owner(&self, key: RoutingKey, owner: String) -> Result<()> {
        self.save_routing_affinity(
            &key,
            &owner,
            self.gateway_settings().await?.sticky_ttl_seconds,
        )
        .await
    }
}

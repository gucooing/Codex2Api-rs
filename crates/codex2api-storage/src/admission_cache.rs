//! Atomic admission for every API key belonging to the same user and product.
use crate::{RuntimePermit, Storage, StorageError};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

#[derive(Clone, Copy, Default)]
pub struct ApiLimits {
    pub group_concurrency: u32,
    pub group_rpm: u32,
    pub model_concurrency: u32,
    pub model_rpm: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("{code}")]
    Limited {
        code: &'static str,
        retry_after: u32,
    },
}

pub struct ApiPermit {
    _permits: Vec<RuntimePermit>,
}

// Check every scope before consuming any RPM window or concurrency slot.
// Sorted sets retain one key per business scope and expire abandoned members.
static ADMIT: LazyLock<redis::Script> = LazyLock::new(|| {
    redis::Script::new(
        r#"
local clock = redis.call('TIME')
local now = clock[1] * 1000 + math.floor(clock[2] / 1000)
for i = 1, 5 do
    local cutoff = now
    if i > 3 then cutoff = now - 60000 end
    redis.call('ZREMRANGEBYSCORE', KEYS[i], '-inf', cutoff)
    local limit = tonumber(ARGV[i])
    local count = redis.call('ZCARD', KEYS[i])
    if limit > 0 and count >= limit then
        local delay = 1
        if i > 3 then
            local oldest = redis.call('ZRANGE', KEYS[i], count-limit, count-limit, 'WITHSCORES')
            delay = math.max(1, math.ceil((tonumber(oldest[2]) + 60000 - now) / 1000))
        end
        return {i,delay}
    end
end
for i = 1, 3 do
    redis.call('ZADD', KEYS[i], now + tonumber(ARGV[9]), ARGV[5+i])
    redis.call('PEXPIRE', KEYS[i], ARGV[9])
end
for i = 4, 5 do
    if tonumber(ARGV[i]) > 0 then
        redis.call('ZADD', KEYS[i], now, ARGV[6])
        redis.call('PEXPIRE', KEYS[i], 60000)
    end
end
return {0,0}
"#,
    )
});

impl Storage {
    pub fn api_key_concurrency(&self, key_id: &str) -> u32 {
        self.runtime.metrics(&format!("api-key:{key_id}")).active
    }
    pub fn api_group_concurrency(&self, user: &str, group: &str) -> u32 {
        self.runtime
            .metrics(&format!("api-group:{user}:{group}"))
            .active
    }
    pub async fn acquire_api_request(
        &self,
        user: &str,
        key_id: &str,
        group: &str,
        model: &str,
        limits: ApiLimits,
    ) -> std::result::Result<ApiPermit, AdmissionError> {
        let group_key = format!("api-group:{user}:{group}");
        let model_key = format!(
            "api-model:{user}:{group}:{:x}",
            Sha256::digest(model.as_bytes())
        );
        let scopes = [
            (&group_key, limits.group_concurrency),
            (&model_key, limits.model_concurrency),
            (&format!("api-key:{key_id}"), 0),
        ];
        let codes = [
            "api_group_concurrency_exceeded",
            "api_model_concurrency_exceeded",
            "api_key_concurrency_exceeded",
            "api_group_rpm_exceeded",
            "api_model_rpm_exceeded",
        ];
        let mut leases = Vec::with_capacity(3);
        for (i, (key, limit)) in scopes.into_iter().enumerate() {
            let lease = self
                .runtime
                .reserve(key, limit)
                .ok_or(AdmissionError::Limited {
                    code: codes[i],
                    retry_after: 1,
                })?;
            leases.push(lease);
        }
        let mut script = ADMIT.prepare_invoke();
        for lease in &leases {
            script.key(lease.redis_key());
        }
        script
            .key(self.runtime.redis.key(&format!("rpm:{group_key}")))
            .key(self.runtime.redis.key(&format!("rpm:{model_key}")));
        script
            .arg(limits.group_concurrency)
            .arg(limits.model_concurrency)
            .arg(0)
            .arg(limits.group_rpm)
            .arg(limits.model_rpm);
        for lease in &leases {
            script.arg(&lease.id);
        }
        script.arg(crate::runtime::LEASE_MS);
        for lease in &leases {
            lease.mark_attempted();
        }
        let (rejected, retry_after): (usize, u32) = self.runtime.redis.script(&script).await?;
        if rejected != 0 {
            return Err(AdmissionError::Limited {
                code: codes[rejected - 1],
                retry_after,
            });
        }
        Ok(ApiPermit {
            _permits: leases
                .into_iter()
                .map(|lease| self.runtime.start_lease(lease))
                .collect(),
        })
    }
}

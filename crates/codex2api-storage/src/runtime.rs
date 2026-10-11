//! Short-lived routing/admission state. SQLite remains the durable business store.
use crate::{Result, StorageError};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const LEASE_MS: i64 = 90_000;
const CAPACITY: usize = 50_000;
const ACQUIRE: &str = r#"
local t = redis.call('TIME')
local now = t[1] * 1000 + math.floor(t[2] / 1000)
redis.call('ZREMRANGEBYSCORE', KEYS[1], '-inf', now)
if tonumber(ARGV[1]) > 0 and redis.call('ZCARD', KEYS[1]) >= tonumber(ARGV[1]) then
    return 0
end
redis.call('ZADD', KEYS[1], now + ARGV[3], ARGV[2])
redis.call('PEXPIRE', KEYS[1], ARGV[3])
return 1
"#;
const RPM: &str = r#"
local now = tonumber(ARGV[1])
redis.call('ZREMRANGEBYSCORE', KEYS[1], '-inf', now - 60000)
local n = redis.call('ZCARD', KEYS[1])
local limit = tonumber(ARGV[2])
if limit > 0 and n >= limit then
    local v = redis.call('ZRANGE', KEYS[1], n - limit, n - limit, 'WITHSCORES')
    return math.max(1, math.ceil((tonumber(v[2]) + 60000 - now) / 1000))
end
if ARGV[3] == '1' and limit > 0 then
    redis.call('ZADD', KEYS[1], now, ARGV[4])
    redis.call('PEXPIRE', KEYS[1], 60000)
end
return 0
"#;

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct RuntimeMetrics {
    pub active: u32,
    pub waiting: u32,
    pub error_rate: f64,
    pub ttft_ms: Option<f64>,
    pub last_used: u64,
}
#[derive(Default, Debug)]
struct Live {
    metrics: RuntimeMetrics,
    rpm: VecDeque<i64>,
    touched: Option<Instant>,
}
#[derive(Debug)]
struct Cached {
    until: Instant,
    value: Arc<serde_json::Value>,
}
pub(crate) struct Runtime {
    redis: Option<redis::aio::ConnectionManager>,
    prefix: String,
    live: [Mutex<HashMap<String, Live>>; 64],
    values: Mutex<HashMap<String, (Instant, String)>>,
    cache: Mutex<HashMap<String, Cached>>,
    generation: AtomicU64,
    sequence: AtomicU64,
    pub(crate) affinity_ttl: AtomicU64,
}
impl Runtime {
    pub async fn new(path: &std::path::Path) -> Result<Arc<Self>> {
        use sha2::{Digest, Sha256};
        let redis = match std::env::var("CODEX2API_REDIS_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
        {
            Some(url) => {
                let client = redis::Client::open(url).map_err(|_| StorageError::Runtime)?;
                Some(
                    tokio::time::timeout(
                        Duration::from_secs(5),
                        redis::aio::ConnectionManager::new(client),
                    )
                    .await
                    .map_err(|_| StorageError::Runtime)?
                    .map_err(|_| StorageError::Runtime)?,
                )
            }
            None => None,
        };
        let absolute = std::path::absolute(path)?;
        let namespace = format!(
            "{:x}",
            Sha256::digest(absolute.to_string_lossy().as_bytes())
        );
        let prefix = std::env::var("CODEX2API_REDIS_PREFIX")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| format!("codex2api:{}", &namespace[..24]));
        Ok(Arc::new(Self {
            redis,
            prefix,
            live: std::array::from_fn(|_| Mutex::new(HashMap::new())),
            values: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
            affinity_ttl: AtomicU64::new(3600),
        }))
    }
    fn shard(&self, key: &str) -> &Mutex<HashMap<String, Live>> {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hash);
        &self.live[hash.finish() as usize % self.live.len()]
    }
    fn key(&self, key: &str) -> String {
        format!("{}:{key}", self.prefix)
    }
    pub fn epoch(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    pub fn invalidate(&self) {
        let mut cache = self.cache.lock().unwrap();
        self.generation.fetch_add(1, Ordering::AcqRel);
        cache.clear();
    }
    pub fn cached<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let value = {
            let cache = self.cache.lock().unwrap();
            let item = cache.get(key)?;
            if item.until <= Instant::now() {
                return None;
            }
            item.value.clone()
        };
        serde_json::from_value((*value).clone()).ok()
    }
    pub fn cache<T: Serialize>(&self, key: String, value: &T, epoch: u64, ttl: Duration) {
        let Ok(value) = serde_json::to_value(value) else {
            return;
        };
        let mut cache = self.cache.lock().unwrap();
        if self.epoch() != epoch {
            return;
        }
        if cache.len() >= CAPACITY {
            cache.retain(|_, v| v.until > Instant::now());
            if cache.len() >= CAPACITY {
                cache.clear();
            }
        }
        cache.insert(
            key,
            Cached {
                until: Instant::now() + ttl,
                value: Arc::new(value),
            },
        );
    }
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        if key.starts_with("response_owner:")
            && let Some((expiry, value)) = self.values.lock().unwrap().get(key)
            && *expiry > Instant::now()
        {
            return Ok(Some(value.clone()));
        }
        if let Some(mut conn) = self.redis.clone() {
            return tokio::time::timeout(
                Duration::from_secs(2),
                redis::cmd("GET").arg(self.key(key)).query_async(&mut conn),
            )
            .await
            .map_err(|_| StorageError::Runtime)?
            .map_err(|_| StorageError::Runtime);
        }
        let mut values = self.values.lock().unwrap();
        if values
            .get(key)
            .is_some_and(|(expiry, _)| *expiry <= Instant::now())
        {
            values.remove(key);
        }
        Ok(values.get(key).map(|(_, v)| v.clone()))
    }
    pub async fn set(&self, key: &str, value: &str, ttl: u64) -> Result<()> {
        if let Some(mut conn) = self.redis.clone() {
            return tokio::time::timeout(
                Duration::from_secs(2),
                redis::cmd("SET")
                    .arg(self.key(key))
                    .arg(value)
                    .arg("EX")
                    .arg(ttl)
                    .query_async(&mut conn),
            )
            .await
            .map_err(|_| StorageError::Runtime)?
            .map_err(|_| StorageError::Runtime);
        }
        let mut values = self.values.lock().unwrap();
        if values.len() >= CAPACITY {
            values.retain(|_, (t, _)| *t > Instant::now());
            if values.len() >= CAPACITY {
                if let Some(key) = values.keys().next().cloned() {
                    values.remove(&key);
                }
            }
        }
        values.insert(
            key.into(),
            (Instant::now() + Duration::from_secs(ttl), value.into()),
        );
        Ok(())
    }
    /// Renew only the mapping that was actually selected, without overwriting a newer one.
    pub async fn refresh(&self, key: &str, id: &str, ttl: u64) -> Result<()> {
        if let Some(mut conn) = self.redis.clone() {
            let _: i64 = tokio::time::timeout(
                Duration::from_secs(2),
                redis::Script::new(
                    r#"
                    if redis.call('GET', KEYS[1]) == ARGV[1] then
                        return redis.call('EXPIRE', KEYS[1], ARGV[2])
                    end
                    return 0
                    "#,
                )
                .key(self.key(key))
                .arg(id)
                .arg(ttl)
                .invoke_async(&mut conn),
            )
            .await
            .map_err(|_| StorageError::Runtime)?
            .map_err(|_| StorageError::Runtime)?;
        } else {
            let mut values = self.values.lock().unwrap();
            if let Some((expiry, value)) = values.get_mut(key)
                && value == id
                && *expiry > Instant::now()
            {
                *expiry = Instant::now() + Duration::from_secs(ttl);
            }
        }
        Ok(())
    }
    /// Debounce last-used timestamps only. Authorization/revocation remain live reads.
    pub fn claim_touch(&self, key: &str) -> bool {
        let mut map = self.shard(key).lock().unwrap();
        if map.len() > CAPACITY / 64 {
            map.retain(|_, v| {
                v.metrics.active > 0
                    || v.metrics.waiting > 0
                    || v.touched
                        .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
            });
        }
        let live = map.entry(key.into()).or_default();
        if live
            .touched
            .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
        {
            return false;
        }
        live.touched = Some(Instant::now());
        true
    }
    /// Compare-and-delete protects a newer selection from an older failed attempt.
    pub async fn forget(&self, key: &str, expected: &str) -> Result<()> {
        if let Some(mut conn) = self.redis.clone() {
            let _: i64 = tokio::time::timeout(
                Duration::from_secs(2),
                redis::Script::new(
                    r#"
                    if redis.call('GET', KEYS[1]) == ARGV[1] then
                        return redis.call('DEL', KEYS[1])
                    end
                    return 0
                    "#,
                )
                .key(self.key(key))
                .arg(expected)
                .invoke_async(&mut conn),
            )
            .await
            .map_err(|_| StorageError::Runtime)?
            .map_err(|_| StorageError::Runtime)?;
        } else {
            let mut values = self.values.lock().unwrap();
            if values.get(key).is_some_and(|(_, v)| v == expected) {
                values.remove(key);
            }
        }
        Ok(())
    }
    pub async fn rpm(
        &self,
        owner: &str,
        limit: u32,
        consume: bool,
        now: i64,
    ) -> Result<Option<u32>> {
        if limit == 0 {
            return Ok(None);
        }
        let key = format!("rpm:{owner}");
        if let Some(mut conn) = self.redis.clone() {
            let delay: i64 = tokio::time::timeout(
                Duration::from_secs(2),
                redis::Script::new(RPM)
                    .key(self.key(&key))
                    .arg(now)
                    .arg(limit)
                    .arg(u8::from(consume))
                    .arg(uuid::Uuid::new_v4().to_string())
                    .invoke_async(&mut conn),
            )
            .await
            .map_err(|_| StorageError::Runtime)?
            .map_err(|_| StorageError::Runtime)?;
            return Ok((delay > 0).then_some(delay as u32));
        }
        let mut map = self.shard(&key).lock().unwrap();
        if map.len() > CAPACITY / 64 {
            map.retain(|_, v| {
                v.metrics.active > 0
                    || v.metrics.waiting > 0
                    || v.touched
                        .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
            });
        }
        let live = map.entry(key).or_default();
        live.touched = Some(Instant::now());
        while live.rpm.front().is_some_and(|v| *v <= now - 60_000) {
            live.rpm.pop_front();
        }
        if live.rpm.len() >= limit as usize {
            return Ok(Some(
                ((live.rpm[live.rpm.len() - limit as usize] + 60_000 - now + 999) / 1000)
                    .clamp(1, 60) as u32,
            ));
        }
        if consume {
            live.rpm.push_back(now);
        }
        Ok(None)
    }
    pub fn metrics(&self, key: &str) -> RuntimeMetrics {
        self.shard(key)
            .lock()
            .unwrap()
            .get(key)
            .map(|v| v.metrics)
            .unwrap_or_default()
    }
    pub fn report(&self, key: &str, success: bool, ttft: Option<f64>) {
        let mut map = self.shard(key).lock().unwrap();
        let live = map.entry(key.into()).or_default();
        live.touched = Some(Instant::now());
        let m = &mut live.metrics;
        m.error_rate = 0.8 * m.error_rate + 0.2 * f64::from(u8::from(!success));
        if let Some(ttft) = ttft.filter(|v| v.is_finite() && *v > 0.0) {
            m.ttft_ms = Some(m.ttft_ms.map_or(ttft, |old| 0.8 * old + 0.2 * ttft));
        }
    }
    pub fn wait(self: &Arc<Self>, key: &str) -> RuntimeWait {
        self.shard(key)
            .lock()
            .unwrap()
            .entry(key.into())
            .or_default()
            .metrics
            .waiting += 1;
        RuntimeWait {
            runtime: self.clone(),
            key: key.into(),
        }
    }
    pub async fn acquire(self: &Arc<Self>, key: &str, limit: u32) -> Result<Option<RuntimePermit>> {
        {
            let mut map = self.shard(key).lock().unwrap();
            let live = map.entry(key.into()).or_default();
            if limit > 0 && live.metrics.active >= limit {
                return Ok(None);
            }
            live.touched = Some(Instant::now());
            live.metrics.active += 1;
            live.metrics.last_used = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        }
        let lease = Arc::new(Lease {
            runtime: self.clone(),
            key: key.into(),
            id: uuid::Uuid::new_v4().to_string(),
            renew: Mutex::new(None),
        });
        if let Some(mut conn) = self.redis.clone() {
            let acquired: i64 = tokio::time::timeout(
                Duration::from_secs(2),
                redis::Script::new(ACQUIRE)
                    .key(self.key(key))
                    .arg(limit)
                    .arg(&lease.id)
                    .arg(LEASE_MS)
                    .invoke_async(&mut conn),
            )
            .await
            .map_err(|_| StorageError::Runtime)?
            .map_err(|_| StorageError::Runtime)?;
            if acquired == 0 {
                return Ok(None);
            }
            let weak = Arc::downgrade(&lease);
            let handle = tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(20)).await;
                    let Some(lease) = weak.upgrade() else { break };
                    let _ = tokio::time::timeout(
                        Duration::from_secs(2),
                        redis::Script::new(
                            r#"
                            local t = redis.call('TIME')
                            local now = t[1] * 1000 + math.floor(t[2] / 1000)
                            redis.call('ZADD', KEYS[1], now + ARGV[2], ARGV[1])
                            redis.call('PEXPIRE', KEYS[1], ARGV[2])
                            return 1
                            "#,
                        )
                        .key(lease.runtime.key(&lease.key))
                        .arg(&lease.id)
                        .arg(LEASE_MS)
                        .invoke_async::<i64>(&mut conn),
                    )
                    .await;
                }
            });
            *lease.renew.lock().unwrap() = Some(handle.abort_handle());
        }
        Ok(Some(RuntimePermit { _lease: lease }))
    }
}
#[derive(Clone, Debug)]
pub struct RuntimePermit {
    _lease: Arc<Lease>,
}
#[derive(Debug)]
struct Lease {
    runtime: Arc<Runtime>,
    key: String,
    id: String,
    renew: Mutex<Option<tokio::task::AbortHandle>>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(handle) = self.renew.lock().unwrap().take() {
            handle.abort();
        }
        if let Some(live) = self
            .runtime
            .shard(&self.key)
            .lock()
            .unwrap()
            .get_mut(&self.key)
        {
            live.metrics.active = live.metrics.active.saturating_sub(1);
        }
        if let (Some(mut conn), Ok(handle)) = (
            self.runtime.redis.clone(),
            tokio::runtime::Handle::try_current(),
        ) {
            let key = self.runtime.key(&self.key);
            let id = self.id.clone();
            handle.spawn(async move {
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    redis::cmd("ZREM")
                        .arg(key)
                        .arg(id)
                        .query_async::<i64>(&mut conn),
                )
                .await;
            });
        }
    }
}
pub(crate) struct RuntimeWait {
    runtime: Arc<Runtime>,
    key: String,
}
impl Drop for RuntimeWait {
    fn drop(&mut self) {
        if let Some(live) = self
            .runtime
            .shard(&self.key)
            .lock()
            .unwrap()
            .get_mut(&self.key)
        {
            live.metrics.waiting = live.metrics.waiting.saturating_sub(1);
        }
    }
}

impl crate::Storage {
    pub fn remember_response_owner(&self, key: String, id: String) {
        let ttl = self.runtime.affinity_ttl.load(Ordering::Acquire);
        {
            let mut values = self.runtime.values.lock().unwrap();
            if values.len() >= CAPACITY {
                values.retain(|_, (t, _)| *t > Instant::now());
                if values.len() >= CAPACITY {
                    if let Some(key) = values.keys().next().cloned() {
                        values.remove(&key);
                    }
                }
            }
            values.insert(
                key.clone(),
                (Instant::now() + Duration::from_secs(ttl), id.clone()),
            );
        }
        let runtime = self.runtime.clone();
        if runtime.redis.is_some() {
            tokio::spawn(async move {
                if runtime.set(&key, &id, ttl).await.is_err() {
                    tracing::warn!("Response affinity persistence unavailable");
                }
            });
        }
    }
    pub fn invalidate_runtime_cache(&self) {
        self.runtime.invalidate();
    }
    pub fn supplier_metrics(&self, id: &str) -> RuntimeMetrics {
        self.runtime.metrics(&format!("supplier:{id}"))
    }
    pub fn report_supplier_result(&self, id: &str, success: bool, ttft: Option<f64>) {
        self.runtime
            .report(&format!("supplier:{id}"), success, ttft);
    }
    pub async fn acquire_supplier(&self, id: &str, limit: u32) -> Result<Option<RuntimePermit>> {
        self.runtime.acquire(&format!("supplier:{id}"), limit).await
    }
    pub async fn acquire_consumer(&self, id: &str) -> Result<Option<RuntimePermit>> {
        self.runtime
            .acquire(
                &format!("consumer:{id}"),
                self.virtual_rpm_limit(id).await?.effective_concurrency,
            )
            .await
    }
    pub fn consumer_active(&self, id: &str) -> u32 {
        self.runtime.metrics(&format!("consumer:{id}")).active
    }
    pub async fn routing_affinity(&self, key: &str) -> Result<Option<String>> {
        self.runtime.get(key).await
    }
    pub async fn save_routing_affinity(&self, key: &str, id: &str, ttl: u64) -> Result<()> {
        self.runtime.set(key, id, ttl).await
    }
    pub async fn refresh_routing_affinity(&self, key: &str, id: &str, ttl: u64) -> Result<()> {
        self.runtime.refresh(key, id, ttl).await
    }
    pub async fn forget_routing_affinity(&self, key: &str, id: &str) -> Result<()> {
        self.runtime.forget(key, id).await
    }
    pub async fn wait_supplier(
        &self,
        id: &str,
        limit: u32,
        timeout: Duration,
    ) -> Result<Option<RuntimePermit>> {
        let key = format!("supplier:{id}");
        let _wait = self.runtime.wait(&key);
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(permit) = self.runtime.acquire(&key, limit).await? {
                return Ok(Some(permit));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("redis_enabled", &self.redis.is_some())
            .finish_non_exhaustive()
    }
}

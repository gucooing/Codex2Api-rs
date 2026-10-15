//! Short-lived routing/admission state. SQLite remains the durable business store.
use crate::{Result, StorageError};
use serde::Serialize;
use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
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
    touched: Option<Instant>,
    available: Arc<tokio::sync::Notify>,
    version: u64,
}
struct Cached {
    until: Instant,
    value: Arc<dyn Any + Send + Sync>,
}
#[derive(Clone)]
struct Snapshot<T> {
    version: u64,
    value: T,
}
#[derive(Default)]
struct SupplierUpdates {
    cursor: u64,
    entries: VecDeque<(u64, String)>,
}
pub(crate) struct Runtime {
    redis: redis::aio::ConnectionManager,
    prefix: String,
    live: [Mutex<HashMap<String, Live>>; 64],
    cache: Mutex<HashMap<String, Cached>>,
    fills: Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>,
    generation: AtomicU64,
    sequence: AtomicU64,
    supplier_updates: Mutex<SupplierUpdates>,
    pub(crate) affinity_ttl: AtomicU64,
}
impl Runtime {
    pub async fn new(path: &std::path::Path) -> Result<Arc<Self>> {
        use sha2::{Digest, Sha256};
        let url = std::env::var("CODEX2API_REDIS_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .ok_or(StorageError::RedisNotConfigured)?;
        let client = redis::Client::open(url).map_err(|_| StorageError::Runtime)?;
        let redis = tokio::time::timeout(
            Duration::from_secs(5),
            redis::aio::ConnectionManager::new(client),
        )
        .await
        .map_err(|_| StorageError::Runtime)?
        .map_err(|_| StorageError::Runtime)?;
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
            cache: Mutex::new(HashMap::new()),
            fills: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
            supplier_updates: Mutex::default(),
            affinity_ttl: AtomicU64::new(3600),
        }))
    }
    fn shard(&self, key: &str) -> &Mutex<HashMap<String, Live>> {
        &self.live[self.shard_index(key)]
    }
    pub(crate) fn shard_index(&self, key: &str) -> usize {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hash);
        hash.finish() as usize % self.live.len()
    }
    fn key(&self, key: &str) -> String {
        format!("{}:{key}", self.prefix)
    }
    pub fn epoch(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    fn supplier_changed(&self, key: &str) {
        let Some(id) = key.strip_prefix("supplier:") else {
            return;
        };
        let mut updates = self.supplier_updates.lock().unwrap();
        updates.cursor += 1;
        let cursor = updates.cursor;
        if updates.entries.len() == CAPACITY {
            updates.entries.pop_front();
        }
        updates.entries.push_back((cursor, id.into()));
    }
    pub(crate) fn supplier_cursor(&self) -> u64 {
        self.supplier_updates.lock().unwrap().cursor
    }
    pub(crate) fn supplier_changes(&self, after: u64) -> Option<(u64, Vec<String>)> {
        let updates = self.supplier_updates.lock().unwrap();
        if updates
            .entries
            .front()
            .is_some_and(|(first, _)| after.saturating_add(1) < *first)
        {
            return None;
        }
        let start = updates
            .entries
            .partition_point(|(cursor, _)| *cursor <= after);
        Some((
            updates.cursor,
            updates
                .entries
                .range(start..)
                .map(|(_, id)| id.clone())
                .collect(),
        ))
    }
    pub fn version(&self, key: &str) -> u64 {
        self.shard(key)
            .lock()
            .unwrap()
            .get(key)
            .map_or(0, |live| live.version)
    }
    pub fn advance_version(&self, key: &str) {
        let mut map = self.shard(key).lock().unwrap();
        let live = map.entry(key.into()).or_default();
        live.version = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        live.touched = Some(Instant::now());
    }
    pub fn invalidate(&self) {
        let mut cache = self.cache.lock().unwrap();
        self.generation.fetch_add(1, Ordering::AcqRel);
        cache.clear();
    }
    pub fn invalidate_key(&self, key: &str) {
        self.advance_version(key);
        let mut cache = self.cache.lock().unwrap();
        cache.remove(key);
    }
    pub async fn snapshot<T: Clone + Send + Sync + 'static, E>(
        &self,
        key: String,
        load: impl std::future::Future<Output = std::result::Result<(T, Duration), E>>,
    ) -> std::result::Result<T, E> {
        let cached = || {
            self.cached::<Snapshot<T>>(&key)
                .filter(|value| value.version == self.version(&key))
                .map(|value| value.value)
        };
        if let Some(value) = cached() {
            return Ok(value);
        }
        let _fill = self.cache_fill(&key).await;
        if let Some(value) = cached() {
            return Ok(value);
        }
        let epoch = self.epoch();
        let version = self.version(&key);
        let (value, ttl) = load.await?;
        self.cache(
            key,
            &Snapshot {
                version,
                value: value.clone(),
            },
            epoch,
            ttl,
        );
        Ok(value)
    }
    pub fn cached<T: Clone + Send + Sync + 'static>(&self, key: &str) -> Option<T> {
        let value = {
            let cache = self.cache.lock().unwrap();
            let item = cache.get(key)?;
            if item.until <= Instant::now() {
                return None;
            }
            item.value.clone()
        };
        value.downcast_ref::<T>().cloned()
    }
    /// A cancelled loader releases the guard; concurrent misses share the next fill.
    pub async fn cache_fill(&self, key: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut fills = self.fills.lock().unwrap();
            if fills.len() >= CAPACITY {
                fills.retain(|_, value| value.strong_count() > 0);
            }
            match fills.get(key).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(tokio::sync::Mutex::new(()));
                    fills.insert(key.into(), Arc::downgrade(&lock));
                    lock
                }
            }
        };
        lock.lock_owned().await
    }
    pub fn cache<T: Clone + Send + Sync + 'static>(
        &self,
        key: String,
        value: &T,
        epoch: u64,
        ttl: Duration,
    ) {
        let value = Arc::new(value.clone());
        let mut cache = self.cache.lock().unwrap();
        if self.epoch() != epoch {
            return;
        }
        if cache.len() >= CAPACITY
            && !cache.contains_key(&key)
            && let Some(old) = cache.keys().next().cloned()
        {
            cache.remove(&old);
        }
        cache.insert(
            key,
            Cached {
                until: Instant::now() + ttl,
                value,
            },
        );
    }
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.redis.clone();
        tokio::time::timeout(
            Duration::from_secs(2),
            redis::cmd("GET").arg(self.key(key)).query_async(&mut conn),
        )
        .await
        .map_err(|_| StorageError::Runtime)?
        .map_err(|_| StorageError::Runtime)
    }
    pub async fn set(&self, key: &str, value: &str, ttl: u64) -> Result<()> {
        let mut conn = self.redis.clone();
        tokio::time::timeout(
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
        .map_err(|_| StorageError::Runtime)
    }
    /// Renew only the mapping that was actually selected, without overwriting a newer one.
    pub async fn refresh(&self, key: &str, id: &str, ttl: u64) -> Result<()> {
        let mut conn = self.redis.clone();
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
        let mut conn = self.redis.clone();
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
        let mut conn = self.redis.clone();
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
        Ok((delay > 0).then_some(delay as u32))
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
        self.supplier_changed(key);
    }
    pub fn wait(self: &Arc<Self>, key: &str) -> RuntimeWait {
        let available = {
            let mut map = self.shard(key).lock().unwrap();
            let live = map.entry(key.into()).or_default();
            live.metrics.waiting += 1;
            self.supplier_changed(key);
            live.available.clone()
        };
        RuntimeWait {
            runtime: self.clone(),
            key: key.into(),
            available,
        }
    }
    pub async fn acquire(self: &Arc<Self>, key: &str, limit: u32) -> Result<Option<RuntimePermit>> {
        let Some(lease) = self.reserve(key, limit) else {
            return Ok(None);
        };
        self.confirm(lease, limit).await
    }
    pub(crate) fn reserve(self: &Arc<Self>, key: &str, limit: u32) -> Option<Arc<Lease>> {
        {
            let mut map = self.shard(key).lock().unwrap();
            let live = map.entry(key.into()).or_default();
            if limit > 0 && live.metrics.active >= limit {
                return None;
            }
            live.touched = Some(Instant::now());
            live.metrics.active += 1;
            live.metrics.last_used = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
            self.supplier_changed(key);
        }
        Some(Arc::new(Lease {
            runtime: self.clone(),
            key: key.into(),
            id: uuid::Uuid::new_v4().to_string(),
            renew: Mutex::new(None),
            acquired: AtomicBool::new(false),
        }))
    }
    pub(crate) async fn confirm(
        self: &Arc<Self>,
        lease: Arc<Lease>,
        limit: u32,
    ) -> Result<Option<RuntimePermit>> {
        let mut conn = self.redis.clone();
        let acquired: i64 = tokio::time::timeout(
            Duration::from_secs(2),
            redis::Script::new(ACQUIRE)
                .key(self.key(&lease.key))
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
        lease.acquired.store(true, Ordering::Release);
        Ok(Some(RuntimePermit { _lease: lease }))
    }
}
#[derive(Clone, Debug)]
pub struct RuntimePermit {
    _lease: Arc<Lease>,
}
#[derive(Debug)]
pub(crate) struct Lease {
    runtime: Arc<Runtime>,
    key: String,
    id: String,
    renew: Mutex<Option<tokio::task::AbortHandle>>,
    acquired: AtomicBool,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(handle) = self.renew.lock().unwrap().take() {
            handle.abort();
        }
        let available = self
            .runtime
            .shard(&self.key)
            .lock()
            .unwrap()
            .get_mut(&self.key)
            .map(|live| {
                live.metrics.active = live.metrics.active.saturating_sub(1);
                self.runtime.supplier_changed(&self.key);
                live.available.clone()
            });
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let mut conn = self.runtime.redis.clone();
            let key = self.runtime.key(&self.key);
            let id = self.id.clone();
            let acquired = self.acquired.load(Ordering::Acquire);
            handle.spawn(async move {
                let released = tokio::time::timeout(
                    Duration::from_secs(2),
                    redis::cmd("ZREM")
                        .arg(key)
                        .arg(id)
                        .query_async::<i64>(&mut conn),
                )
                .await;
                if acquired
                    && matches!(released, Ok(Ok(_)))
                    && let Some(available) = available
                {
                    available.notify_waiters();
                }
            });
        }
    }
}
pub(crate) struct RuntimeWait {
    runtime: Arc<Runtime>,
    key: String,
    available: Arc<tokio::sync::Notify>,
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
            self.runtime.supplier_changed(&self.key);
        }
    }
}

impl crate::Storage {
    pub async fn remember_response_owner(&self, key: String, id: String) -> Result<()> {
        let ttl = self.runtime.affinity_ttl.load(Ordering::Acquire);
        self.runtime.set(&key, &id, ttl).await
    }
    pub fn invalidate_runtime_cache(&self) {
        self.runtime.invalidate();
    }
    pub fn supplier_metrics(&self, id: &str) -> RuntimeMetrics {
        self.runtime.metrics(&format!("supplier:{id}"))
    }
    /// Lock each occupied shard once, without caching live concurrency counts.
    pub fn supplier_metrics_batch(
        &self,
        accounts: &[Arc<crate::PoolCandidate>],
    ) -> Vec<RuntimeMetrics> {
        let mut offsets = [0usize; 65];
        for account in accounts {
            offsets[account.runtime_shard + 1] += 1;
        }
        for shard in 1..offsets.len() {
            offsets[shard] += offsets[shard - 1];
        }
        let mut positions = offsets;
        let mut indices = vec![0usize; accounts.len()];
        for (index, account) in accounts.iter().enumerate() {
            indices[positions[account.runtime_shard]] = index;
            positions[account.runtime_shard] += 1;
        }
        let mut result = vec![RuntimeMetrics::default(); accounts.len()];
        for (shard_index, shard) in self.runtime.live.iter().enumerate() {
            let indices = &indices[offsets[shard_index]..offsets[shard_index + 1]];
            if indices.is_empty() {
                continue;
            }
            let map = shard.lock().unwrap();
            for &index in indices {
                if let Some(live) = map.get(&accounts[index].runtime_key) {
                    result[index] = live.metrics;
                }
            }
        }
        result
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
        let wait = self.runtime.wait(&key);
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let available = wait.available.notified();
            tokio::pin!(available);
            // Register before checking the count so a concurrent release cannot be lost.
            available.as_mut().enable();
            if let Some(permit) = self.runtime.acquire(&key, limit).await? {
                return Ok(Some(permit));
            }
            tokio::select! {
                _ = &mut available => {},
                _ = tokio::time::sleep_until(deadline) => return Ok(None),
                // Other processes do not signal our local Notify.
                _ = tokio::time::sleep(Duration::from_millis(50)) => {},
            }
        }
    }
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn snapshots_share_values_and_reject_pre_invalidation_fills() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(&dir.path().join("cache.sqlite"))
            .await
            .unwrap();
        let epoch = runtime.epoch();
        let snapshot = Arc::new(vec!["snapshot".to_string()]);
        runtime.cache("shared".into(), &snapshot, epoch, Duration::from_secs(30));
        let read: Arc<Vec<String>> = runtime.cached("shared").unwrap();
        assert!(Arc::ptr_eq(&snapshot, &read));
        runtime.invalidate();
        runtime.cache("shared".into(), &snapshot, epoch, Duration::from_secs(30));
        assert!(runtime.cached::<Arc<Vec<String>>>("shared").is_none());
        assert_eq!(read[0], "snapshot");
    }

    #[tokio::test]
    async fn cancelled_cache_loader_does_not_block_the_next_loader() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(&dir.path().join("cache.sqlite"))
            .await
            .unwrap();
        let (ready, waiting) = tokio::sync::oneshot::channel();
        let loader = runtime.clone();
        let task = tokio::spawn(async move {
            let _guard = loader.cache_fill("key").await;
            ready.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        waiting.await.unwrap();
        task.abort();
        let _ = task.await;
        let _guard = tokio::time::timeout(Duration::from_secs(1), runtime.cache_fill("key"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn released_slots_notify_waiters_and_failed_acquisitions_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(&dir.path().join("slots.sqlite"))
            .await
            .unwrap();
        let permit = runtime.acquire("supplier:a", 1).await.unwrap().unwrap();
        let wait = runtime.wait("supplier:a");
        let available = wait.available.clone();
        let notification = available.notified();
        tokio::pin!(notification);
        notification.as_mut().enable();
        assert!(runtime.acquire("supplier:a", 1).await.unwrap().is_none());
        assert!(
            tokio::time::timeout(Duration::from_millis(5), &mut notification)
                .await
                .is_err()
        );
        drop(permit);
        tokio::time::timeout(Duration::from_secs(1), notification)
            .await
            .unwrap();
        let acquired = runtime.acquire("supplier:a", 1).await.unwrap().unwrap();
        assert_eq!(runtime.metrics("supplier:a").active, 1);
        drop(acquired);
        drop(wait);
        assert_eq!(runtime.metrics("supplier:a").active, 0);
        assert_eq!(runtime.metrics("supplier:a").waiting, 0);
    }
}

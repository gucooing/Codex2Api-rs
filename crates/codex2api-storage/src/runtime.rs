//! Short-lived routing/admission state. PostgreSQL remains the durable business store.
use crate::Result;
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub(crate) const LEASE_MS: i64 = 90_000;
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
// Every window is checked before any window consumes; ARGV holds one limit per key,
// then the consume flag and the member identity.
const RPM: &str = r#"
local clock = redis.call('TIME')
local now = clock[1] * 1000 + math.floor(clock[2] / 1000)
local count = #KEYS
for i = 1, count do
    local limit = tonumber(ARGV[i])
    if limit > 0 then
        redis.call('ZREMRANGEBYSCORE', KEYS[i], '-inf', now - 60000)
        local n = redis.call('ZCARD', KEYS[i])
        if n >= limit then
            local v = redis.call('ZRANGE', KEYS[i], n - limit, n - limit, 'WITHSCORES')
            return math.max(1, math.ceil((tonumber(v[2]) + 60000 - now) / 1000))
        end
    end
end
if ARGV[count + 1] == '1' then
    for i = 1, count do
        if tonumber(ARGV[i]) > 0 then
            redis.call('ZADD', KEYS[i], now, ARGV[count + 2])
            redis.call('PEXPIRE', KEYS[i], 60000)
        end
    end
end
return 0
"#;

static ACQUIRE_SCRIPT: std::sync::LazyLock<redis::Script> =
    std::sync::LazyLock::new(|| redis::Script::new(ACQUIRE));
static RPM_SCRIPT: std::sync::LazyLock<redis::Script> =
    std::sync::LazyLock::new(|| redis::Script::new(RPM));
static RENEW_SCRIPT: std::sync::LazyLock<redis::Script> = std::sync::LazyLock::new(|| {
    redis::Script::new(
        r#"
local clock = redis.call('TIME')
local now = clock[1] * 1000 + math.floor(clock[2] / 1000)
redis.call('ZADD', KEYS[1], now + ARGV[2], ARGV[1])
redis.call('PEXPIRE', KEYS[1], ARGV[2])
return 1
"#,
    )
});

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
}
#[derive(Default)]
struct SupplierUpdates {
    cursor: u64,
    entries: VecDeque<(u64, String)>,
}
pub(crate) struct Runtime {
    pub(crate) redis: crate::redis_store::RedisStore,
    pub(crate) snapshots: crate::snapshots::Snapshots,
    live: [Mutex<HashMap<String, Live>>; 64],
    sequence: AtomicU64,
    supplier_updates: Mutex<SupplierUpdates>,
}
impl Runtime {
    pub async fn new(identity: &str) -> Result<Arc<Self>> {
        let redis = crate::redis_store::RedisStore::connect(identity).await?;
        Ok(Arc::new(Self {
            snapshots: crate::snapshots::Snapshots::new(redis.clone()),
            redis,
            live: std::array::from_fn(|_| Mutex::new(HashMap::new())),
            sequence: AtomicU64::new(0),
            supplier_updates: Mutex::default(),
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
    /// Rolling-minute windows admitted atomically; a zero limit is an unlimited scope.
    pub async fn rpm_scopes(&self, scopes: &[(String, u32)], consume: bool) -> Result<Option<u32>> {
        if scopes.iter().all(|(_, limit)| *limit == 0) {
            return Ok(None);
        }
        let mut script = RPM_SCRIPT.prepare_invoke();
        for (key, _) in scopes {
            script.key(self.redis.key(key));
        }
        for (_, limit) in scopes {
            script.arg(*limit);
        }
        script
            .arg(u8::from(consume))
            .arg(uuid::Uuid::new_v4().to_string());
        let delay: i64 = self.redis.script(&script).await?;
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
            attempted: AtomicBool::new(false),
        }))
    }
    pub(crate) async fn confirm(
        self: &Arc<Self>,
        lease: Arc<Lease>,
        limit: u32,
    ) -> Result<Option<RuntimePermit>> {
        lease.mark_attempted();
        let acquired: i64 = self
            .redis
            .script(
                &ACQUIRE_SCRIPT
                    .key(lease.redis_key())
                    .arg(limit)
                    .arg(&lease.id)
                    .arg(LEASE_MS),
            )
            .await?;
        if acquired == 0 {
            return Ok(None);
        }
        Ok(Some(self.start_lease(lease)))
    }
    pub(crate) fn start_lease(self: &Arc<Self>, lease: Arc<Lease>) -> RuntimePermit {
        let weak = Arc::downgrade(&lease);
        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(20)).await;
                let Some(lease) = weak.upgrade() else { break };
                let _ = lease
                    .runtime
                    .redis
                    .script::<i64>(
                        &RENEW_SCRIPT
                            .key(lease.redis_key())
                            .arg(&lease.id)
                            .arg(LEASE_MS),
                    )
                    .await;
            }
        });
        *lease.renew.lock().unwrap() = Some(handle.abort_handle());
        lease.acquired.store(true, Ordering::Release);
        RuntimePermit { _lease: lease }
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
    pub(crate) id: String,
    renew: Mutex<Option<tokio::task::AbortHandle>>,
    acquired: AtomicBool,
    attempted: AtomicBool,
}
impl Lease {
    pub(crate) fn redis_key(&self) -> String {
        self.runtime.redis.key(&format!("concurrency:{}", self.key))
    }
    pub(crate) fn mark_attempted(&self) {
        self.attempted.store(true, Ordering::Release);
    }
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
        if self.attempted.load(Ordering::Acquire)
            && let Ok(handle) = tokio::runtime::Handle::try_current()
        {
            let redis = self.runtime.redis.clone();
            let key = self.redis_key();
            let id = self.id.clone();
            let acquired = self.acquired.load(Ordering::Acquire);
            handle.spawn(async move {
                let released = redis
                    .command::<i64>(redis::cmd("ZREM").arg(key).arg(id))
                    .await;
                if acquired
                    && released.is_ok()
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
    async fn released_slots_notify_waiters_and_failed_acquisitions_do_not() {
        let runtime = Runtime::new(&uuid::Uuid::new_v4().to_string())
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

//! Bounded local hot values over Redis snapshots, with coalesced database loads.
//! Generations are values, never part of Redis keys. A process incarnation rejects
//! snapshots left before a crash between a database commit and invalidation.
use crate::{Result, redis_store::RedisStore};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    any::Any,
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

const CAPACITY: usize = 50_000;
const HOT_TTL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Domain {
    Suppliers,
    Pools,
    Catalog,
    Billing,
    Subscriptions,
    Settings,
    ApiGroups,
}

fn domain(key: &str) -> Domain {
    match key.split(':').next().unwrap_or(key) {
        "pool-index" | "pool-rows" | "pool-union" => Domain::Pools,
        "supplier-account"
        | "supplier-auth"
        | "supplier-auth-revision"
        | "supplier-routing"
        | "supplier-transport"
        | "supplier-models" => Domain::Suppliers,
        "model-catalog" | "model-config" => Domain::Catalog,
        "billing-snapshot" => Domain::Billing,
        "gateway-settings" => Domain::Settings,
        "api-group" => Domain::ApiGroups,
        "entitlements" | "execution-route" | "request-limits" | "spending-decision" => {
            Domain::Subscriptions
        }
        _ => panic!("unregistered snapshot family: {key}"),
    }
}

struct Entry {
    until: Instant,
    stamp: u64,
    value: Arc<dyn Any + Send + Sync>,
}
#[derive(Default)]
struct State {
    serial: u64,
    domains: HashMap<Domain, u64>,
    versions: HashMap<String, u64>,
    values: HashMap<String, Entry>,
    fills: HashMap<String, Weak<tokio::sync::Mutex<()>>>,
}
impl State {
    fn stamp(&self, key: &str) -> u64 {
        self.domains
            .get(&domain(key))
            .copied()
            .unwrap_or(0)
            .max(self.versions.get(key).copied().unwrap_or(0))
    }
}

#[derive(Serialize, Deserialize)]
struct Envelope<T> {
    incarnation: String,
    stamp: u64,
    expires_at_ms: i64,
    value: T,
}

pub(crate) struct Snapshots {
    redis: RedisStore,
    incarnation: String,
    state: Mutex<State>,
}
impl Snapshots {
    pub fn new(redis: RedisStore) -> Self {
        Self {
            redis,
            incarnation: uuid::Uuid::new_v4().to_string(),
            state: Mutex::default(),
        }
    }
    pub fn policy_revision(&self) -> u64 {
        let state = self.state.lock().unwrap();
        [Domain::Subscriptions, Domain::Billing, Domain::Catalog]
            .iter()
            .filter_map(|domain| state.domains.get(domain))
            .copied()
            .max()
            .unwrap_or(0)
    }
    pub fn version(&self, key: &str) -> u64 {
        self.state.lock().unwrap().stamp(key)
    }
    pub fn invalidate(&self, domains: &[Domain]) {
        let mut state = self.state.lock().unwrap();
        state.serial += 1;
        let revision = state.serial;
        for scope in domains {
            state.domains.insert(*scope, revision);
        }
        state
            .values
            .retain(|key, _| !domains.contains(&domain(key)));
        state
            .versions
            .retain(|key, _| !domains.contains(&domain(key)));
    }
    pub fn invalidate_key(&self, key: &str) {
        let mut state = self.state.lock().unwrap();
        state.serial += 1;
        let revision = state.serial;
        state.versions.insert(key.to_owned(), revision);
        state.values.remove(key);
    }
    pub fn get<T: Clone + Send + Sync + 'static>(&self, key: &str) -> Option<T> {
        let mut state = self.state.lock().unwrap();
        let entry = state.values.get(key)?;
        if entry.until <= Instant::now() || entry.stamp != state.stamp(key) {
            state.values.remove(key);
            return None;
        }
        let value = entry.value.clone();
        drop(state);
        value.downcast_ref::<T>().cloned()
    }
    pub fn put<T: Clone + Send + Sync + 'static>(
        &self,
        key: String,
        value: &T,
        stamp: u64,
        ttl: Duration,
    ) {
        let until = Instant::now() + ttl;
        let mut state = self.state.lock().unwrap();
        if state.stamp(&key) != stamp || ttl.is_zero() {
            return;
        }
        if state.values.len() >= CAPACITY
            && !state.values.contains_key(&key)
            && let Some(old) = state.values.keys().next().cloned()
        {
            state.values.remove(&old);
        }
        state.values.insert(
            key,
            Entry {
                until,
                stamp,
                value: Arc::new(value.clone()),
            },
        );
    }
    pub async fn fill(&self, key: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut state = self.state.lock().unwrap();
            if state.fills.len() >= CAPACITY {
                state.fills.retain(|_, lock| lock.strong_count() > 0);
            }
            match state.fills.get(key).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(tokio::sync::Mutex::new(()));
                    state.fills.insert(key.into(), Arc::downgrade(&lock));
                    lock
                }
            }
        };
        lock.lock_owned().await
    }
    pub async fn read<T: Clone + Serialize + DeserializeOwned + Send + Sync + 'static>(
        &self,
        key: String,
        load: impl std::future::Future<Output = Result<(T, Duration)>>,
    ) -> Result<T> {
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        let _fill = self.fill(&key).await;
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        let mut stamp = self.version(&key);
        let redis_key = self.redis.key(&format!("cache:{key}"));
        let bytes: Option<Vec<u8>> = self
            .redis
            .command(redis::cmd("GET").arg(&redis_key))
            .await?;
        if let Some(bytes) = bytes {
            let entry: Envelope<T> = serde_json::from_slice(&bytes)?;
            let remaining = entry
                .expires_at_ms
                .saturating_sub(chrono::Utc::now().timestamp_millis());
            if entry.incarnation == self.incarnation
                && entry.stamp == stamp
                && remaining > 0
                && self.version(&key) == stamp
            {
                self.put(
                    key,
                    &entry.value,
                    stamp,
                    HOT_TTL.min(Duration::from_millis(remaining as u64)),
                );
                return Ok(entry.value);
            }
        }
        stamp = self.version(&key);
        let (value, ttl) = load.await?;
        if !ttl.is_zero() && self.version(&key) == stamp {
            // Downward jitter avoids synchronized misses without extending a business deadline.
            let ttl_ms = ttl.as_millis().min(i64::MAX as u128) as u64;
            let ttl_ms = ttl_ms
                .saturating_sub(rand::random::<u64>() % (ttl_ms / 10 + 1))
                .max(1);
            let entry = Envelope {
                incarnation: self.incarnation.clone(),
                stamp,
                expires_at_ms: chrono::Utc::now()
                    .timestamp_millis()
                    .saturating_add(ttl_ms as i64),
                value: &value,
            };
            // Unknown identifiers can be client-controlled; negative lookups stay
            // in the bounded hot cache instead of creating Redis keys.
            if serde_json::to_vec(&value)? != b"null" {
                self.redis
                    .command::<()>(
                        redis::cmd("SET")
                            .arg(redis_key)
                            .arg(serde_json::to_vec(&entry)?)
                            .arg("PX")
                            .arg(ttl_ms),
                    )
                    .await?;
            }
            self.put(
                key,
                &value,
                stamp,
                HOT_TTL.min(Duration::from_millis(
                    entry
                        .expires_at_ms
                        .saturating_sub(chrono::Utc::now().timestamp_millis())
                        .max(0) as u64,
                )),
            );
        }
        Ok(value)
    }
}

impl crate::Storage {
    pub fn invalidate_cached_snapshots(&self) {
        use Domain::*;
        self.runtime.snapshots.invalidate(&[
            Suppliers,
            Pools,
            Catalog,
            Billing,
            Subscriptions,
            Settings,
            ApiGroups,
        ]);
    }
}

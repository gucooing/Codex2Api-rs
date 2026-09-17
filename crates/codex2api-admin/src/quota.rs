//! SQLite quota snapshots, refreshed only when requested.
use chrono::{TimeDelta, Utc};
use codex2api_storage::{QuotaSnapshot, Storage};
use serde_json::Value;
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::sync::Mutex;

const CACHE_TTL: TimeDelta = TimeDelta::minutes(10);

pub(crate) struct QuotaCache {
    storage: Storage,
    // Only request locks live in memory; every snapshot is read from SQLite.
    refresh_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl QuotaCache {
    pub(crate) fn new(storage: Storage) -> Self {
        Self {
            storage,
            refresh_locks: Mutex::new(HashMap::new()),
        }
    }

    async fn refresh_lock(&self, account_id: &str) -> Arc<Mutex<()>> {
        self.refresh_locks
            .lock()
            .await
            .entry(account_id.to_string())
            .or_default()
            .clone()
    }

    pub(crate) async fn get_or_fetch<F, Fut>(
        &self,
        account_id: &str,
        refresh: bool,
        fetch: F,
    ) -> Result<QuotaSnapshot, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Value, String>>,
    {
        let lock = self.refresh_lock(account_id).await;
        let _guard = lock.lock().await;
        if !refresh
            && let Some(snapshot) = self
                .storage
                .get_account_quota(account_id)
                .await
                .map_err(|error| error.to_string())?
        {
            let age = Utc::now() - snapshot.observed_at;
            if age >= TimeDelta::zero() && age < CACHE_TTL {
                return Ok(snapshot);
            }
        }
        let snapshot = QuotaSnapshot {
            value: fetch().await?,
            observed_at: Utc::now(),
        };
        self.storage
            .store_account_quota(account_id, &snapshot)
            .await
            .map_err(|error| error.to_string())?;
        Ok(snapshot)
    }

    pub(crate) async fn invalidate(&self, account_id: &str) {
        let lock = self.refresh_lock(account_id).await;
        let _guard = lock.lock().await;
        if let Err(error) = self.storage.delete_account_quota(account_id).await {
            tracing::warn!(account_id, %error, "failed to invalidate quota cache");
        }
    }

    pub(crate) async fn evict(&self, account_id: &str) {
        self.refresh_locks.lock().await.remove(account_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex2api_accounts::AccountStore;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn setup() -> (tempfile::TempDir, Storage, QuotaCache, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("quota.sqlite"))
            .await
            .unwrap();
        let accounts = AccountStore::open(storage.clone());
        let a = accounts.create_pending().await.unwrap().account.id;
        let b = accounts.create_pending().await.unwrap().account.id;
        let cache = QuotaCache::new(storage.clone());
        (dir, storage, cache, a, b)
    }

    #[tokio::test]
    async fn snapshots_survive_restart_and_are_always_read_from_sqlite() {
        let (_dir, storage, cache, a, _) = setup().await;
        let first = cache
            .get_or_fetch(&a, false, || async { Ok(json!(42)) })
            .await
            .unwrap();
        assert_eq!(
            storage.get_account_quota(&a).await.unwrap(),
            Some(first.clone())
        );
        let path = storage.db_path().to_path_buf();
        storage.close().await;
        drop(cache);
        drop(storage);

        let storage = Storage::open(path).await.unwrap();
        let cache = QuotaCache::new(storage.clone());
        let cached = cache
            .get_or_fetch(&a, false, || async {
                panic!("restart must reuse SQLite snapshot")
            })
            .await
            .unwrap();
        assert_eq!(cached, first);

        let updated = QuotaSnapshot {
            value: json!(43),
            observed_at: Utc::now(),
        };
        storage.store_account_quota(&a, &updated).await.unwrap();
        let cached = cache
            .get_or_fetch(&a, false, || async { panic!("fresh database snapshot") })
            .await
            .unwrap();
        assert_eq!(cached, updated);
        storage.delete_account(&a).await.unwrap();
        assert!(storage.get_account_quota(&a).await.unwrap().is_none());
        storage.close().await;
    }

    #[tokio::test]
    async fn refreshes_expired_cache_only_on_access_and_isolates_accounts() {
        let (_dir, storage, cache, a, b) = setup().await;
        let calls = AtomicUsize::new(0);
        let fetch = || async { Ok(json!(calls.fetch_add(1, Ordering::SeqCst))) };
        let mut first = cache.get_or_fetch(&a, false, fetch).await.unwrap();
        first.observed_at = Utc::now() - TimeDelta::minutes(9);
        storage.store_account_quota(&a, &first).await.unwrap();
        assert_eq!(cache.get_or_fetch(&a, false, fetch).await.unwrap(), first);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let other = cache.get_or_fetch(&b, false, fetch).await.unwrap();
        assert_ne!(other.value, first.value);
        first.observed_at = Utc::now() - TimeDelta::minutes(10);
        storage.store_account_quota(&a, &first).await.unwrap();
        assert_eq!(
            storage.get_account_quota(&a).await.unwrap(),
            Some(first.clone())
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let renewed = cache.get_or_fetch(&a, false, fetch).await.unwrap();
        assert_ne!(renewed.value, first.value);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(storage.get_account_quota(&b).await.unwrap(), Some(other));
        storage.close().await;
    }

    #[tokio::test]
    async fn manual_refresh_persists_new_snapshot_and_failure_preserves_it() {
        let (_dir, storage, cache, a, _) = setup().await;
        let first = QuotaSnapshot {
            value: json!(42),
            observed_at: Utc::now() - TimeDelta::minutes(9),
        };
        storage.store_account_quota(&a, &first).await.unwrap();
        let manual = cache
            .get_or_fetch(&a, true, || async { Ok(json!(43)) })
            .await
            .unwrap();
        assert_eq!(manual.value, json!(43));
        assert!(manual.observed_at > first.observed_at);
        assert_eq!(
            storage.get_account_quota(&a).await.unwrap(),
            Some(manual.clone())
        );
        assert!(
            cache
                .get_or_fetch(&a, true, || async { Err("offline".into()) })
                .await
                .is_err()
        );
        assert_eq!(
            storage.get_account_quota(&a).await.unwrap(),
            Some(manual.clone())
        );
        assert_eq!(
            cache
                .get_or_fetch(&a, false, || async { panic!("cache should survive") })
                .await
                .unwrap(),
            manual
        );
        cache.invalidate(&a).await;
        assert!(storage.get_account_quota(&a).await.unwrap().is_none());
        let renewed = cache
            .get_or_fetch(&a, false, || async { Ok(json!(44)) })
            .await
            .unwrap();
        assert_eq!(renewed.value, json!(44));
        storage.close().await;
    }

    #[tokio::test]
    async fn concurrent_reads_share_a_fetch_without_blocking_other_accounts() {
        let (_dir, storage, cache, a, b) = setup().await;
        let other_finished = tokio::sync::Notify::new();
        let (first, second, other) =
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                tokio::join!(
                    cache.get_or_fetch(&a, false, || async {
                        other_finished.notified().await;
                        Ok(json!("a"))
                    }),
                    cache
                        .get_or_fetch(&a, false, || async { panic!("duplicate upstream request") }),
                    async {
                        let other = cache
                            .get_or_fetch(&b, false, || async { Ok(json!("b")) })
                            .await;
                        other_finished.notify_one();
                        other
                    }
                )
            })
            .await
            .unwrap();
        assert_eq!(first.unwrap(), second.unwrap());
        assert_eq!(other.unwrap().value, json!("b"));
        storage.close().await;
    }
}

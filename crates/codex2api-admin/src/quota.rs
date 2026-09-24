//! Account-isolated SQLite snapshots for supplier information.
use chrono::{TimeDelta, Utc};
use codex2api_storage::{QuotaSnapshot, Storage, SupplierInfoSection};
use serde_json::{Value, json};
use std::{collections::HashMap, future::Future, sync::Arc};
use tokio::sync::Mutex;

pub(crate) const CACHE_TTL: TimeDelta = TimeDelta::minutes(10);

/// Keep every reported window and its duration; primary is not always five hours.
pub(crate) fn summary(snapshot: &QuotaSnapshot) -> Value {
    let limits = snapshot.value.get("rate_limit");
    let windows: Vec<Value> = ["primary_window", "secondary_window"]
        .into_iter()
        .filter_map(|key| {
            let v = limits?.get(key).filter(|v| v.is_object())?;
            let seconds = v
                .get("limit_window_seconds")
                .and_then(Value::as_i64)
                .filter(|seconds| *seconds > 0);
            let used = v
                .get("used_percent")
                .and_then(Value::as_f64)
                .filter(|p| p.is_finite() && *p >= 0.0);
            let reset = v
                .get("reset_at")
                .and_then(Value::as_i64)
                .filter(|t| *t > 0)
                .or_else(|| {
                    v.get("reset_after_seconds")
                        .and_then(Value::as_i64)
                        .filter(|t| *t >= 0)
                        .and_then(|seconds| snapshot.observed_at.timestamp().checked_add(seconds))
                });
            Some(json!({
                "id": key,
                "limit_window_seconds": seconds,
                "used_percent": used,
                "reset_at": reset,
            }))
        })
        .collect();
    json!({"observed_at":snapshot.observed_at,"stale":Utc::now()-snapshot.observed_at>=CACHE_TTL,
        "windows":windows})
}

pub(crate) struct SupplierCache {
    storage: Storage,
    // Only request locks live in memory; every snapshot is read from SQLite.
    refresh_locks: Mutex<HashMap<(String, SupplierInfoSection), Arc<Mutex<()>>>>,
}

impl SupplierCache {
    pub(crate) fn new(storage: Storage) -> Self {
        Self {
            storage,
            refresh_locks: Mutex::new(HashMap::new()),
        }
    }

    async fn refresh_lock(&self, account_id: &str, section: SupplierInfoSection) -> Arc<Mutex<()>> {
        self.refresh_locks
            .lock()
            .await
            .entry((account_id.to_string(), section))
            .or_default()
            .clone()
    }

    pub(crate) async fn get_or_fetch<F, Fut>(
        &self,
        account_id: &str,
        section: SupplierInfoSection,
        refresh: bool,
        fetch: F,
    ) -> Result<QuotaSnapshot, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Value, String>>,
    {
        let lock = self.refresh_lock(account_id, section).await;
        let _guard = lock.lock().await;
        if !refresh
            && let Some(snapshot) = self
                .storage
                .get_supplier_info(account_id, section)
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
            .store_supplier_info(account_id, section, &snapshot)
            .await
            .map_err(|error| error.to_string())?;
        Ok(snapshot)
    }

    pub(crate) async fn invalidate(&self, account_id: &str, section: SupplierInfoSection) {
        let lock = self.refresh_lock(account_id, section).await;
        let _guard = lock.lock().await;
        if let Err(error) = self.storage.delete_supplier_info(account_id, section).await {
            tracing::warn!(account_id, %error, "failed to invalidate quota cache");
        }
    }

    pub(crate) async fn invalidate_all(&self, account_id: &str) {
        for section in SupplierInfoSection::ALL {
            self.invalidate(account_id, section).await;
        }
    }

    pub(crate) async fn evict(&self, account_id: &str) {
        self.refresh_locks
            .lock()
            .await
            .retain(|(id, _), _| id != account_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex2api_accounts::SupplierAccountStore;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn setup() -> (tempfile::TempDir, Storage, SupplierCache, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("quota.sqlite"))
            .await
            .unwrap();
        let accounts = SupplierAccountStore::open(storage.clone());
        let a = accounts.create_pending().await.unwrap().account.id;
        let b = accounts.create_pending().await.unwrap().account.id;
        let cache = SupplierCache::new(storage.clone());
        (dir, storage, cache, a, b)
    }

    #[tokio::test]
    async fn snapshots_survive_restart_and_are_always_read_from_sqlite() {
        let (_dir, storage, cache, a, _) = setup().await;
        let first = cache
            .get_or_fetch(&a, SupplierInfoSection::Quota, false, || async {
                Ok(json!(42))
            })
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
        let cache = SupplierCache::new(storage.clone());
        let cached = cache
            .get_or_fetch(&a, SupplierInfoSection::Quota, false, || async {
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
            .get_or_fetch(&a, SupplierInfoSection::Quota, false, || async {
                panic!("fresh database snapshot")
            })
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
        let mut first = cache
            .get_or_fetch(&a, SupplierInfoSection::Quota, false, fetch)
            .await
            .unwrap();
        first.observed_at = Utc::now() - TimeDelta::minutes(9);
        storage.store_account_quota(&a, &first).await.unwrap();
        assert_eq!(
            cache
                .get_or_fetch(&a, SupplierInfoSection::Quota, false, fetch)
                .await
                .unwrap(),
            first
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let other = cache
            .get_or_fetch(&b, SupplierInfoSection::Quota, false, fetch)
            .await
            .unwrap();
        assert_ne!(other.value, first.value);
        first.observed_at = Utc::now() - TimeDelta::minutes(10);
        storage.store_account_quota(&a, &first).await.unwrap();
        assert_eq!(
            storage.get_account_quota(&a).await.unwrap(),
            Some(first.clone())
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let renewed = cache
            .get_or_fetch(&a, SupplierInfoSection::Quota, false, fetch)
            .await
            .unwrap();
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
            .get_or_fetch(&a, SupplierInfoSection::Quota, true, || async {
                Ok(json!(43))
            })
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
                .get_or_fetch(&a, SupplierInfoSection::Quota, true, || async {
                    Err("offline".into())
                })
                .await
                .is_err()
        );
        assert_eq!(
            storage.get_account_quota(&a).await.unwrap(),
            Some(manual.clone())
        );
        assert_eq!(
            cache
                .get_or_fetch(&a, SupplierInfoSection::Quota, false, || async {
                    panic!("cache should survive")
                })
                .await
                .unwrap(),
            manual
        );
        cache.invalidate(&a, SupplierInfoSection::Quota).await;
        assert!(storage.get_account_quota(&a).await.unwrap().is_none());
        let renewed = cache
            .get_or_fetch(&a, SupplierInfoSection::Quota, false, || async {
                Ok(json!(44))
            })
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
                    cache.get_or_fetch(&a, SupplierInfoSection::Quota, false, || async {
                        other_finished.notified().await;
                        Ok(json!("a"))
                    }),
                    cache.get_or_fetch(&a, SupplierInfoSection::Quota, false, || async {
                        panic!("duplicate upstream request")
                    }),
                    async {
                        let other = cache
                            .get_or_fetch(&b, SupplierInfoSection::Quota, false, || async {
                                Ok(json!("b"))
                            })
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

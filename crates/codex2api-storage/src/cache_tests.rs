use crate::{RoutingKey, admission_cache::*, redis_store::RedisStore, snapshots::*};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[tokio::test]
async fn concurrent_cache_misses_share_a_load_and_redis_survives_hot_eviction() {
    let redis = RedisStore::connect(&uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    let cache = Arc::new(Snapshots::new(redis.clone()));
    let loads = Arc::new(AtomicUsize::new(0));
    let key = "api-group:one";
    let mut tasks = Vec::new();
    for _ in 0..32 {
        let cache = cache.clone();
        let loads = loads.clone();
        tasks.push(tokio::spawn(async move {
            cache
                .read(key.into(), async {
                    loads.fetch_add(1, Ordering::Relaxed);
                    tokio::task::yield_now().await;
                    Ok((42u64, Duration::from_secs(30)))
                })
                .await
                .unwrap()
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap(), 42);
    }
    assert_eq!(loads.load(Ordering::Relaxed), 1);
    // Expire only the local hot copy; a valid L2 value must avoid another DB load.
    cache.put(
        key.into(),
        &99u64,
        cache.version(key),
        Duration::from_nanos(1),
    );
    let value = cache
        .read(key.into(), async {
            loads.fetch_add(1, Ordering::Relaxed);
            Ok((0u64, Duration::from_secs(30)))
        })
        .await
        .unwrap();
    assert_eq!(value, 42);
    assert_eq!(loads.load(Ordering::Relaxed), 1);
    let ttl: i64 = redis
        .command(redis::cmd("PTTL").arg(redis.key("cache:api-group:one")))
        .await
        .unwrap();
    assert!((1..=30000).contains(&ttl));
}

#[tokio::test]
async fn invalidation_fences_inflight_loads_without_evicting_other_domains() {
    let redis = RedisStore::connect(&uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    let cache = Arc::new(Snapshots::new(redis.clone()));
    let key = "supplier-account:one";
    cache
        .read("billing-snapshot:chatgpt".into(), async {
            Ok((7u64, Duration::from_secs(30)))
        })
        .await
        .unwrap();
    let (ready, waiting) = tokio::sync::oneshot::channel();
    let (release, blocked) = tokio::sync::oneshot::channel();
    let reader = cache.clone();
    let task = tokio::spawn(async move {
        reader
            .read(key.into(), async {
                ready.send(()).unwrap();
                blocked.await.unwrap();
                Ok((1u64, Duration::from_secs(30)))
            })
            .await
            .unwrap()
    });
    waiting.await.unwrap();
    cache.invalidate_key(key);
    release.send(()).unwrap();
    task.await.unwrap();
    assert!(cache.get::<u64>(key).is_none());
    assert_eq!(cache.get::<u64>("billing-snapshot:chatgpt"), Some(7));
    assert_eq!(
        cache
            .read(key.into(), async { Ok((2u64, Duration::from_secs(30))) })
            .await
            .unwrap(),
        2
    );
    cache.invalidate(&[Domain::Suppliers]);
    assert_eq!(
        cache
            .read(key.into(), async { Ok((3u64, Duration::from_secs(30))) })
            .await
            .unwrap(),
        3
    );
    // A fresh process cannot revive snapshots written before an interrupted invalidation.
    let restarted = Snapshots::new(redis);
    assert_eq!(
        restarted
            .read(key.into(), async { Ok((4u64, Duration::from_secs(30))) })
            .await
            .unwrap(),
        4
    );
}

#[tokio::test]
async fn negative_identifiers_do_not_expand_redis_and_cancelled_loaders_release_waiters() {
    let redis = RedisStore::connect(&uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    let cache = Arc::new(Snapshots::new(redis.clone()));
    for i in 0..64 {
        let value: Option<u64> = cache
            .read(format!("model-config:chatgpt:unknown-{i}"), async {
                Ok((None, Duration::from_secs(30)))
            })
            .await
            .unwrap();
        assert_eq!(value, None);
    }
    let (_, keys): (u64, Vec<String>) = redis
        .command(
            redis::cmd("SCAN")
                .arg(0)
                .arg("MATCH")
                .arg(redis.key("cache:*"))
                .arg("COUNT")
                .arg(10000),
        )
        .await
        .unwrap();
    assert!(keys.is_empty());
    let (ready, waiting) = tokio::sync::oneshot::channel();
    let blocked = cache.clone();
    let task = tokio::spawn(async move {
        let _guard = blocked.fill("api-group:cancelled").await;
        ready.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    waiting.await.unwrap();
    task.abort();
    let _ = task.await;
    let value = tokio::time::timeout(
        Duration::from_secs(1),
        cache.read("api-group:cancelled".into(), async {
            Ok((1u64, Duration::from_secs(30)))
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(value, 1);
}

#[tokio::test]
async fn api_scopes_admit_atomically_across_keys_users_and_runtime_instances() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let path = dir.path().join("admission-pg");
    let storage = codex2api_storage::test_support::open(&path).await.unwrap();
    let limits = ApiLimits {
        group_concurrency: 2,
        group_rpm: 2,
        model_rpm: 1,
        ..Default::default()
    };
    let first = storage
        .acquire_api_request("alice", "key1", "group", "one", limits)
        .await
        .unwrap();
    assert!(matches!(
        storage
            .acquire_api_request("alice", "key2", "group", "one", limits)
            .await,
        Err(AdmissionError::Limited {
            code: "api_model_rpm_exceeded",
            ..
        })
    ));
    // A rejected model admission must not consume the remaining group RPM.
    let second = storage
        .acquire_api_request("alice", "key2", "group", "two", limits)
        .await
        .unwrap();
    let other = codex2api_storage::test_support::open(&path).await.unwrap();
    assert!(matches!(
        other
            .acquire_api_request("alice", "key3", "group", "three", limits)
            .await,
        Err(AdmissionError::Limited {
            code: "api_group_concurrency_exceeded",
            ..
        })
    ));
    let bob = other
        .acquire_api_request("bob", "key4", "group", "one", limits)
        .await
        .unwrap();
    assert_eq!(storage.api_key_concurrency("key1"), 1);
    drop((first, second, bob));
    assert_eq!(storage.api_key_concurrency("key1"), 0);
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match other
                .acquire_api_request("alice", "key3", "group", "three", limits)
                .await
            {
                Err(AdmissionError::Limited {
                    code: "api_group_rpm_exceeded",
                    ..
                }) => break,
                Err(AdmissionError::Limited {
                    code: "api_group_concurrency_exceeded",
                    ..
                }) => tokio::task::yield_now().await,
                _ => panic!("scope accounting changed after release"),
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn parallel_api_admission_holds_exactly_the_shared_capacity() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("parallel-pg"))
        .await
        .unwrap();
    let mut tasks = Vec::new();
    for i in 0..24 {
        let storage = storage.clone();
        tasks.push(tokio::spawn(async move {
            storage
                .acquire_api_request(
                    "a",
                    &format!("key{i}"),
                    "g",
                    "model",
                    ApiLimits {
                        group_concurrency: 3,
                        ..Default::default()
                    },
                )
                .await
        }));
    }
    let mut permits = Vec::new();
    for task in tasks {
        if let Ok(permit) = task.await.unwrap() {
            permits.push(permit);
        }
    }
    assert_eq!(permits.len(), 3);
    assert_eq!(storage.api_group_concurrency("a", "g"), 3);
    drop(permits);
    assert_eq!(storage.api_group_concurrency("a", "g"), 0);
}

#[tokio::test]
async fn routing_batches_preserve_ownership_and_compare_updates() {
    let dir = codex2api_storage::test_support::tempdir().unwrap();
    let storage = codex2api_storage::test_support::open(dir.path().join("routing-pg"))
        .await
        .unwrap();
    let response = RoutingKey::response("user-a", "chatgpt", "resp_same");
    let turn = RoutingKey::turn("user-a", "chatgpt", "secret-state");
    let stranger = RoutingKey::response("user-b", "chatgpt", "resp_same");
    let channel = RoutingKey::response("user-a", "grok", "resp_same");
    storage
        .save_routing_affinities(&[(&response, "old"), (&turn, "old")], 60)
        .await
        .unwrap();
    assert_eq!(
        storage
            .routing_affinities([
                Some(&response),
                Some(&turn),
                Some(&stranger),
                Some(&channel),
                None
            ])
            .await
            .unwrap(),
        [Some("old".into()), Some("old".into()), None, None, None]
    );
    storage
        .save_routing_affinity(&response, "new", 60)
        .await
        .unwrap();
    storage
        .forget_routing_affinity(&response, "old")
        .await
        .unwrap();
    storage
        .refresh_routing_affinity(&response, "old", 1)
        .await
        .unwrap();
    assert_eq!(
        storage
            .routing_affinity(&response)
            .await
            .unwrap()
            .as_deref(),
        Some("new")
    );
    storage
        .forget_routing_affinity(&response, "new")
        .await
        .unwrap();
    assert!(storage.routing_affinity(&response).await.unwrap().is_none());
}

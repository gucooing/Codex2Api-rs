//! Atomic, per-user API group and model admissions. Supplier capacity belongs to the scheduler.
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Counter {
    active: u32,
    requests: VecDeque<Instant>,
}

impl Counter {
    fn prune(&mut self, now: Instant) {
        while self
            .requests
            .front()
            .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(60))
        {
            self.requests.pop_front();
        }
    }
    fn check(
        &self,
        concurrency: u32,
        rpm: u32,
        now: Instant,
        model: bool,
    ) -> Result<(), LimitError> {
        if concurrency > 0 && self.active >= concurrency {
            return Err(LimitError {
                code: if model {
                    "api_model_concurrency_exceeded"
                } else {
                    "api_group_concurrency_exceeded"
                },
                retry_after: 1,
            });
        }
        if rpm > 0 && self.requests.len() >= rpm as usize {
            let remaining = self.requests.front().map_or(1, |at| {
                60_000u128.saturating_sub(now.duration_since(*at).as_millis())
            });
            return Err(LimitError {
                code: if model {
                    "api_model_rpm_exceeded"
                } else {
                    "api_group_rpm_exceeded"
                },
                retry_after: remaining.div_ceil(1000).max(1) as u32,
            });
        }
        Ok(())
    }
}

type GroupKey = (String, String);
type ModelKey = (String, String, String);

#[derive(Default)]
struct State {
    groups: HashMap<GroupKey, Counter>,
    models: HashMap<ModelKey, Counter>,
    keys: HashMap<String, u32>,
}

#[derive(Clone, Default)]
pub struct GatewayRuntime(Arc<Mutex<State>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitError {
    pub code: &'static str,
    pub retry_after: u32,
}

#[derive(Clone, Copy, Default)]
pub struct Limits {
    pub group_concurrency: u32,
    pub group_rpm: u32,
    pub model_concurrency: u32,
    pub model_rpm: u32,
}

pub struct ApiPermit {
    runtime: GatewayRuntime,
    group: GroupKey,
    model: ModelKey,
    key_id: String,
}

impl GatewayRuntime {
    pub fn current_for_key(&self, key_id: &str) -> u32 {
        self.0
            .lock()
            .unwrap()
            .keys
            .get(key_id)
            .copied()
            .unwrap_or(0)
    }
    pub fn current_for_user_group(&self, user_id: &str, group_id: &str) -> u32 {
        self.0
            .lock()
            .unwrap()
            .groups
            .get(&(user_id.into(), group_id.into()))
            .map_or(0, |counter| counter.active)
    }
    pub fn acquire(
        &self,
        user_id: &str,
        key_id: &str,
        group_id: &str,
        model: &str,
        limits: Limits,
    ) -> Result<ApiPermit, LimitError> {
        self.acquire_at(user_id, key_id, group_id, model, limits, Instant::now())
    }
    fn acquire_at(
        &self,
        user_id: &str,
        key_id: &str,
        group_id: &str,
        model: &str,
        limits: Limits,
        now: Instant,
    ) -> Result<ApiPermit, LimitError> {
        let group = (user_id.to_owned(), group_id.to_owned());
        let model = (user_id.to_owned(), group_id.to_owned(), model.to_owned());
        let mut state = self.0.lock().unwrap();
        // Remove only inactive, expired windows; active leases are never evicted.
        state.groups.retain(|_, counter| {
            counter.prune(now);
            counter.active > 0 || !counter.requests.is_empty()
        });
        state.models.retain(|_, counter| {
            counter.prune(now);
            counter.active > 0 || !counter.requests.is_empty()
        });
        state.groups.entry(group.clone()).or_default().check(
            limits.group_concurrency,
            limits.group_rpm,
            now,
            false,
        )?;
        state.models.entry(model.clone()).or_default().check(
            limits.model_concurrency,
            limits.model_rpm,
            now,
            true,
        )?;
        // Both checks must pass before either RPM window is consumed.
        let counter = state.groups.get_mut(&group).unwrap();
        counter.active += 1;
        counter.requests.push_back(now);
        let counter = state.models.get_mut(&model).unwrap();
        counter.active += 1;
        counter.requests.push_back(now);
        *state.keys.entry(key_id.into()).or_default() += 1;
        Ok(ApiPermit {
            runtime: self.clone(),
            group,
            model,
            key_id: key_id.into(),
        })
    }
}

impl Drop for ApiPermit {
    fn drop(&mut self) {
        let mut state = self.runtime.0.lock().unwrap();
        if let Some(counter) = state.groups.get_mut(&self.group) {
            counter.active = counter.active.saturating_sub(1);
        }
        if let Some(counter) = state.models.get_mut(&self.model) {
            counter.active = counter.active.saturating_sub(1);
        }
        if let Some(count) = state.keys.get_mut(&self.key_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.keys.remove(&self.key_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_limits_are_shared_across_keys_and_isolated_between_users() {
        let runtime = GatewayRuntime::default();
        let limits = Limits {
            group_concurrency: 1,
            ..Default::default()
        };
        let permit = runtime
            .acquire("alice", "key1", "group", "one", limits)
            .unwrap();
        assert_eq!(
            runtime
                .acquire("alice", "key2", "group", "two", limits)
                .err()
                .unwrap()
                .code,
            "api_group_concurrency_exceeded"
        );
        let bob = runtime
            .acquire("bob", "key3", "group", "one", limits)
            .unwrap();
        assert_eq!(runtime.current_for_key("key1"), 1);
        drop(permit);
        assert_eq!(runtime.current_for_key("key1"), 0);
        assert!(
            runtime
                .acquire("alice", "key2", "group", "two", limits)
                .is_ok()
        );
        drop(bob);
    }
    #[test]
    fn model_limits_compose_atomically_with_group_and_rpm_expires() {
        let runtime = GatewayRuntime::default();
        let now = Instant::now();
        let limits = Limits {
            group_rpm: 2,
            model_rpm: 1,
            ..Default::default()
        };
        drop(
            runtime
                .acquire_at("a", "k1", "g", "one", limits, now)
                .unwrap(),
        );
        assert_eq!(
            runtime
                .acquire_at("a", "k2", "g", "one", limits, now)
                .err()
                .unwrap()
                .code,
            "api_model_rpm_exceeded"
        );
        drop(
            runtime
                .acquire_at("a", "k2", "g", "two", limits, now)
                .unwrap(),
        );
        assert_eq!(
            runtime
                .acquire_at("a", "k2", "g", "three", limits, now)
                .err()
                .unwrap()
                .code,
            "api_group_rpm_exceeded"
        );
        assert!(
            runtime
                .acquire_at("a", "k2", "g", "one", limits, now + Duration::from_secs(60))
                .is_ok()
        );
    }
}

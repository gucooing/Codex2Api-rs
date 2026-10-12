//! Model-pool ready indexes. Requests update changed accounts instead of rescoring the pool.
use crate::{PoolCandidate, Result, RuntimeMetrics, Storage};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Copy, Debug)]
struct Number(f64);
impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Number {}
impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Number {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}
type LoadOrder = (i32, u64, u64);
type ScoreOrder = (Number, i32, u64, u32, usize);

struct ReadyIndex {
    pool: Arc<Vec<Arc<PoolCandidate>>>,
    ids: HashMap<String, usize>,
    metrics: Vec<RuntimeMetrics>,
    load_groups: BTreeMap<LoadOrder, Vec<usize>>,
    positions: Vec<usize>,
    scored: BTreeSet<ScoreOrder>,
    ttft: BTreeMap<Number, usize>,
    waiting: BTreeMap<u32, usize>,
    priority: (i32, i32),
    cursor: u64,
}
fn count<K: Ord>(map: &mut BTreeMap<K, usize>, key: K, add: bool) {
    if add {
        *map.entry(key).or_default() += 1;
    } else if let Some(value) = map.get_mut(&key) {
        *value -= 1;
        if *value == 0 {
            map.remove(&key);
        }
    }
}
impl ReadyIndex {
    fn new(storage: &Storage, pool: Arc<Vec<Arc<PoolCandidate>>>) -> Self {
        let cursor = storage.runtime.supplier_cursor();
        let metrics = storage.supplier_metrics_batch(&pool);
        let priority = (
            pool.iter().map(|a| a.priority).min().unwrap_or(0),
            pool.iter().map(|a| a.priority).max().unwrap_or(0),
        );
        let ids = pool
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id.clone(), i))
            .collect();
        let mut value = Self {
            positions: vec![0; pool.len()],
            pool,
            ids,
            metrics,
            load_groups: BTreeMap::new(),
            scored: BTreeSet::new(),
            ttft: BTreeMap::new(),
            waiting: BTreeMap::new(),
            priority,
            cursor,
        };
        for metrics in &value.metrics {
            if let Some(ttft) = metrics.ttft_ms {
                count(&mut value.ttft, Number(ttft), true);
            }
            count(&mut value.waiting, metrics.waiting, true);
        }
        for i in 0..value.pool.len() {
            value.insert(i);
        }
        value
    }
    fn bounds(&self) -> (Option<Number>, Option<Number>, u32) {
        (
            self.ttft.first_key_value().map(|(n, _)| *n),
            self.ttft.last_key_value().map(|(n, _)| *n),
            self.waiting
                .last_key_value()
                .map_or(1, |(n, _)| (*n).max(1)),
        )
    }
    fn ready(&self, i: usize) -> bool {
        self.pool[i].concurrency == 0 || self.metrics[i].active < self.pool[i].concurrency
    }
    fn load_order(&self, i: usize) -> LoadOrder {
        let account = &self.pool[i];
        let metrics = &self.metrics[i];
        let capacity = account
            .load_factor
            .filter(|n| *n > 0)
            .unwrap_or(account.concurrency.max(1));
        (
            account.priority,
            100 * (u64::from(metrics.active) + u64::from(metrics.waiting)) / u64::from(capacity),
            metrics.last_used,
        )
    }
    fn score_order(&self, i: usize) -> ScoreOrder {
        let (priority, load, _) = self.load_order(i);
        let m = self.metrics[i];
        let (min_t, max_t, max_q) = self.bounds();
        let p = if self.priority.0 == self.priority.1 {
            1.0
        } else {
            1.0 - (f64::from(priority) - f64::from(self.priority.0))
                / (f64::from(self.priority.1) - f64::from(self.priority.0))
        };
        let t = match (m.ttft_ms, min_t, max_t) {
            (Some(t), Some(min), Some(max)) if max > min => {
                1.0 - ((t - min.0) / (max.0 - min.0)).clamp(0.0, 1.0)
            }
            _ => 0.5,
        };
        let score = p + 1.0 - (load as f64 / 100.0).clamp(0.0, 1.0)
            + 0.7 * (1.0 - f64::from(m.waiting) / f64::from(max_q))
            + 0.8 * (1.0 - m.error_rate.clamp(0.0, 1.0))
            + 0.5 * t;
        (Number(-score), priority, load, m.waiting, i)
    }
    fn insert(&mut self, i: usize) {
        if !self.ready(i) {
            return;
        }
        let key = self.load_order(i);
        let group = self.load_groups.entry(key).or_default();
        self.positions[i] = group.len();
        group.push(i);
        self.scored.insert(self.score_order(i));
    }
    fn remove(&mut self, i: usize) {
        if !self.ready(i) {
            return;
        }
        self.scored.remove(&self.score_order(i));
        let key = self.load_order(i);
        if let Some(group) = self.load_groups.get_mut(&key) {
            group.swap_remove(self.positions[i]);
            if let Some(moved) = group.get(self.positions[i]) {
                self.positions[*moved] = self.positions[i];
            }
            if group.is_empty() {
                self.load_groups.remove(&key);
            }
        }
    }
    fn update(&mut self, storage: &Storage) {
        let Some((cursor, changes)) = storage.runtime.supplier_changes(self.cursor) else {
            *self = Self::new(storage, self.pool.clone());
            return;
        };
        self.cursor = cursor;
        let changed: HashSet<usize> = changes
            .iter()
            .filter_map(|id| self.ids.get(id).copied())
            .collect();
        if changed.is_empty() {
            return;
        }
        let previous_bounds = self.bounds();
        for &i in &changed {
            self.remove(i);
        }
        for &i in &changed {
            let old = self.metrics[i];
            let next = storage.supplier_metrics(&self.pool[i].id);
            if let Some(t) = old.ttft_ms {
                count(&mut self.ttft, Number(t), false);
            }
            if let Some(t) = next.ttft_ms {
                count(&mut self.ttft, Number(t), true);
            }
            count(&mut self.waiting, old.waiting, false);
            count(&mut self.waiting, next.waiting, true);
            self.metrics[i] = next;
        }
        if self.bounds() != previous_bounds {
            self.scored.clear();
            for i in 0..self.pool.len() {
                if self.ready(i) {
                    self.scored.insert(self.score_order(i));
                }
            }
        }
        for i in changed {
            self.insert(i);
        }
    }
    fn pick(
        &self,
        traditional: bool,
        top_k: usize,
        previous: Option<&str>,
        sticky: Option<&str>,
        excluded: &HashSet<String>,
        random: f64,
    ) -> Option<Arc<PoolCandidate>> {
        if traditional {
            for group in self.load_groups.values() {
                let offset = ((random * group.len() as f64) as usize).min(group.len() - 1);
                if let Some(&i) = group[offset..]
                    .iter()
                    .chain(&group[..offset])
                    .find(|&&i| !excluded.contains(&self.pool[i].id))
                {
                    return Some(self.pool[i].clone());
                }
            }
            return None;
        }
        let top_k = top_k.max(1);
        let mut choices: Vec<_> = self
            .scored
            .iter()
            .filter(|key| !excluded.contains(&self.pool[key.4].id))
            .take(top_k)
            .copied()
            .collect();
        for id in [previous, sticky].into_iter().flatten() {
            if let Some(&i) = self.ids.get(id)
                && self.ready(i)
                && !excluded.contains(id)
                && !choices.iter().any(|key| key.4 == i)
            {
                choices.push(self.score_order(i));
            }
        }
        for key in &mut choices {
            let id = self.pool[key.4].id.as_str();
            if previous == Some(id) {
                key.0.0 -= 5.0;
            }
            if sticky == Some(id) {
                key.0.0 -= 3.0;
            }
        }
        choices.sort_unstable();
        choices.truncate(top_k);
        for id in [previous, sticky].into_iter().flatten() {
            if let Some(key) = choices.iter().find(|key| self.pool[key.4].id == id) {
                return Some(self.pool[key.4].clone());
            }
        }
        let minimum = -choices.last()?.0.0;
        let sum: f64 = choices.iter().map(|key| -key.0.0 - minimum + 1.0).sum();
        let mut target = random * sum;
        for key in &choices {
            target -= -key.0.0 - minimum + 1.0;
            if target < 0.0 {
                return Some(self.pool[key.4].clone());
            }
        }
        choices.last().map(|key| self.pool[key.4].clone())
    }
}
impl Storage {
    pub async fn acquire_ready_supplier(
        &self,
        pool: Arc<Vec<Arc<PoolCandidate>>>,
        traditional: bool,
        top_k: usize,
        previous: Option<&str>,
        sticky: Option<&str>,
        excluded: &HashSet<String>,
        random: f64,
        reserve: bool,
    ) -> Result<Option<(Arc<PoolCandidate>, Option<crate::RuntimePermit>)>> {
        let key = format!("dispatch:{:p}", Arc::as_ptr(&pool));
        let index: Arc<Mutex<ReadyIndex>> = self
            .runtime
            .snapshot(key, async {
                Ok::<_, crate::StorageError>((
                    Arc::new(Mutex::new(ReadyIndex::new(self, pool))),
                    Duration::from_secs(30),
                ))
            })
            .await?;
        let (candidate, lease) = {
            let mut index = index.lock().unwrap();
            loop {
                index.update(self);
                let candidate = if excluded
                    .iter()
                    .any(|id| index.ids.get(id).is_some_and(|&i| index.ready(i)))
                {
                    // Only retry exclusions change the policy's normalization population.
                    let pool = Arc::new(
                        index
                            .pool
                            .iter()
                            .filter(|a| !excluded.contains(&a.id))
                            .cloned()
                            .collect(),
                    );
                    ReadyIndex::new(self, pool).pick(
                        traditional,
                        top_k,
                        previous,
                        sticky,
                        excluded,
                        random,
                    )
                } else {
                    index.pick(traditional, top_k, previous, sticky, excluded, random)
                };
                let Some(candidate) = candidate else {
                    return Ok(None);
                };
                if !reserve {
                    return Ok(Some((candidate, None)));
                }
                if let Some(lease) = self
                    .runtime
                    .reserve(&candidate.runtime_key, candidate.concurrency)
                {
                    break (candidate, lease);
                }
            }
        };
        let permit = self.runtime.confirm(lease, candidate.concurrency).await?;
        Ok(Some((candidate, permit)))
    }
}

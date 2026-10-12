//! Long-lived account resources. Unrelated accounts never wait on a construction lock.
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, RwLock},
};

struct Slot<T> {
    value: RwLock<Option<T>>,
    update: tokio::sync::Mutex<()>,
}
pub struct AccountCache<T> {
    slots: RwLock<HashMap<String, Arc<Slot<T>>>>,
}
impl<T> Default for AccountCache<T> {
    fn default() -> Self {
        Self {
            slots: RwLock::default(),
        }
    }
}
impl<T: Clone> AccountCache<T> {
    pub async fn get_or_update<E, F: Future<Output = Result<T, E>>>(
        &self,
        id: &str,
        current: impl Fn(&T) -> bool,
        build: impl FnOnce(Option<T>) -> F,
    ) -> Result<T, E> {
        let existing = self.slots.read().unwrap().get(id).cloned();
        let slot = existing.unwrap_or_else(|| {
            self.slots
                .write()
                .unwrap()
                .entry(id.into())
                .or_insert_with(|| {
                    Arc::new(Slot {
                        value: RwLock::new(None),
                        update: tokio::sync::Mutex::new(()),
                    })
                })
                .clone()
        });
        if let Some(value) = slot.value.read().unwrap().as_ref().filter(|v| current(v)) {
            return Ok(value.clone());
        }
        let _update = slot.update.lock().await;
        let previous = slot.value.read().unwrap().clone();
        if let Some(value) = previous.as_ref().filter(|v| current(v)) {
            return Ok(value.clone());
        }
        let value = build(previous).await?;
        *slot.value.write().unwrap() = Some(value.clone());
        Ok(value)
    }
    pub fn remove(&self, id: &str) {
        self.slots.write().unwrap().remove(id);
    }
}

//! Recovery checks are independent of administrator browser activity.
use crate::AdminState;
use std::{future::Future, time::Duration};
use tokio::task::JoinSet;

const PROBE_INTERVAL: Duration = Duration::from_secs(10 * 60);
const PROBE_CONCURRENCY: usize = 4;

impl AdminState {
    /// Started once by the process entry point using the administrator's shared
    /// account clients and quota cache. Router construction never starts tasks.
    pub async fn monitor_supplier_quota(self) {
        loop {
            match self.storage.supplier_quota_probe_candidates(codex2api_core::CHATGPT).await {
                Ok(ids) => probe_candidates(ids, |id| {
                    let state = self.clone();
                    async move {
                        if let Err(error) = probe(&state, &id).await {
                            tracing::warn!(supplier_id=%id, %error, "supplier quota recovery check failed");
                        }
                    }
                }).await,
                Err(error) => tracing::warn!(%error, "failed to read exhausted suppliers"),
            }
            // Failed checks also wait before retrying. Never overlap sweeps or
            // burst through missed ticks after slow official responses.
            tokio::time::sleep(PROBE_INTERVAL).await;
        }
    }
}

async fn probe(state: &AdminState, id: &str) -> Result<(), String> {
    // A queued supplier may have been reset, disabled or rejected in the meantime.
    let account = state
        .storage
        .require_account(id)
        .await
        .map_err(|e| e.to_string())?;
    let health = state
        .storage
        .supplier_health(id)
        .await
        .map_err(|e| e.to_string())?;
    if account.status != codex2api_storage::SupplierStatus::Active
        || health.authentication_invalid
        || health.payment_required
        || health.cooldown_kind.as_deref() != Some("quota_exhausted")
    {
        return Ok(());
    }
    crate::providers::quota(state, id, true).await?;
    Ok(())
}

async fn probe_candidates<F, Fut>(ids: Vec<String>, probe: F)
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let mut tasks = JoinSet::new();
    for id in ids {
        if tasks.len() >= PROBE_CONCURRENCY {
            let _ = tasks.join_next().await;
        }
        tasks.spawn(probe(id));
    }
    while tasks.join_next().await.is_some() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test]
    async fn probes_every_selected_supplier_with_bounded_concurrency() {
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let finished = Arc::new(AtomicUsize::new(0));
        probe_candidates((0..17).map(|n| n.to_string()).collect(), |_| {
            let (running, peak, finished) = (running.clone(), peak.clone(), finished.clone());
            async move {
                peak.fetch_max(running.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                tokio::task::yield_now().await;
                running.fetch_sub(1, Ordering::SeqCst);
                finished.fetch_add(1, Ordering::SeqCst);
            }
        })
        .await;
        assert_eq!(finished.load(Ordering::SeqCst), 17);
        assert!(peak.load(Ordering::SeqCst) <= PROBE_CONCURRENCY);
    }
}

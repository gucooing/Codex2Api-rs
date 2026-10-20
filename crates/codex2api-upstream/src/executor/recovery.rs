//! One bounded recovery policy for HTTP and WebSocket attempts. Transports only
//! carry out the returned retry, switch or rejection. Same-supplier retries are each
//! channel's verdict; how many times a request moves to another supplier is the
//! administrator's request retry setting, shared by every channel and transport.
use super::ExecutorError;
use crate::{
    UpstreamPool,
    supply::{UpstreamFailure, Verdict, Wire},
};
use codex2api_storage::{Storage, SupplierAccount};
use std::{collections::HashMap, time::Duration};

#[derive(Debug, PartialEq, Eq)]
pub enum Recovery {
    Retry(Duration),
    Switch,
}

#[derive(Default)]
pub struct Attempts {
    excluded: Vec<String>,
    retries: HashMap<String, u32>,
    switches: u32,
}

impl Attempts {
    pub fn excluded(&self) -> &[String] {
        &self.excluded
    }

    /// Unsupported conversions never consume the switch budget.
    pub fn exclude(&mut self, id: &str) {
        if !self.excluded.iter().any(|old| old == id) {
            self.excluded.push(id.into());
        }
    }

    fn resolve(&mut self, id: &str, verdict: Verdict) -> (Verdict, Option<Duration>) {
        match verdict {
            Verdict::RetrySame {
                delay,
                max,
                fallback,
            } => {
                let count = self.retries.entry(id.into()).or_default();
                if *count < max {
                    *count += 1;
                    let delay = delay
                        .saturating_mul(1 << count.saturating_sub(1).min(4))
                        .min(Duration::from_secs(8));
                    (*fallback, Some(delay))
                } else {
                    (*fallback, None)
                }
            }
            verdict => (verdict, None),
        }
    }

    /// Give up on the supplier; `retries` is how many switches one request may make.
    fn switch(&mut self, id: &str, retries: u32) -> Result<Recovery, ExecutorError> {
        self.exclude(id);
        self.switches += 1;
        if self.switches > retries {
            Err(ExecutorError::NoSupplier {
                model_not_found: false,
            })
        } else {
            Ok(Recovery::Switch)
        }
    }

    pub async fn failed(
        &mut self,
        pool: &UpstreamPool,
        storage: &Storage,
        account: &SupplierAccount,
        revision: Option<i64>,
        wire: Wire,
        failure: &UpstreamFailure,
    ) -> Result<Recovery, ExecutorError> {
        // An attempt with output is never replayed, but its availability evidence
        // still applies to later requests.
        let mut evidence = std::borrow::Cow::Borrowed(failure);
        if failure.output_committed {
            evidence.to_mut().output_committed = false;
        }
        let verdict = pool.verdict(account, &evidence).await;
        let (verdict, retry) = self.resolve(&account.id, verdict);
        if !failure.output_committed
            && let Some(delay) = retry
        {
            storage.report_supplier_result(&account.id, false, None);
            return Ok(Recovery::Retry(delay));
        }
        crate::supply::apply_verdict(storage, &account.id, revision, &verdict).await?;
        if failure.output_committed {
            return Err(ExecutorError::Stopped {
                status: failure.status.unwrap_or(502),
                body: failure.body.clone(),
                wire,
            });
        }
        match verdict {
            Verdict::ClientError { status, body } => {
                Err(ExecutorError::Client { status, body, wire })
            }
            Verdict::Stop { status, body } => Err(ExecutorError::Stopped { status, body, wire }),
            Verdict::Avoid { .. } => {
                let retries = storage.gateway_settings().await?.request_retries;
                let recovery = self.switch(&account.id, retries)?;
                storage.report_supplier_result(&account.id, false, None);
                Ok(recovery)
            }
            Verdict::RetrySame { .. } => unreachable!("channel fallback must be terminal"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_request_switches_at_most_the_configured_retries() {
        for retries in [0, 3] {
            let mut attempts = Attempts::default();
            // An unsupported conversion is never a candidate and costs no retry.
            attempts.exclude("unsupported");
            for n in 0..retries {
                assert_eq!(
                    attempts.switch(&format!("supplier-{n}"), retries).unwrap(),
                    Recovery::Switch
                );
            }
            assert!(matches!(
                attempts.switch("last", retries),
                Err(ExecutorError::NoSupplier {
                    model_not_found: false
                })
            ));
            assert_eq!(attempts.excluded().len(), retries as usize + 2);
        }
    }

    #[test]
    fn retries_are_bounded_per_supplier_and_backoff_is_capped() {
        let mut attempts = Attempts::default();
        let verdict = || Verdict::retry_same(Duration::from_secs(3), 3, Verdict::avoid_request());
        for expected in [Some(3), Some(6), Some(8), None] {
            let (_, delay) = attempts.resolve("a", verdict());
            assert_eq!(delay.map(|v| v.as_secs()), expected);
        }
        assert_eq!(
            attempts.resolve("b", verdict()).1,
            Some(Duration::from_secs(3))
        );
    }
}

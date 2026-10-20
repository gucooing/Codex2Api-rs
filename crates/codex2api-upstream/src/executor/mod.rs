//! The executor: select a supplier, convert the downstream request into that
//! supplier's wire, send it, and act on the channel's verdict until a reply with
//! output is in hand or every candidate is unavailable. Downstream listeners own
//! admission, settlement and output formatting; channels own transport and verdicts.
mod inspect;
mod recovery;
pub use recovery::{Attempts, Recovery};

use crate::{
    UpstreamError, UpstreamPool, convert,
    inbound::InboundRequest,
    scheduler::{self, ScheduleRequest, SchedulerError, SupplierContext, SupplierRoute},
    supply::{Operation, UpstreamFailure, Wire},
};
use codex2api_storage::{Storage, SupplierAccount};
pub use inspect::{generation_observed, prelude_event};

/// Affinity seeds for supplier selection. A route pinned to one wire uses that
/// wire's rules; a route open to every wire uses the public fields all wires share.
pub fn schedule(wire: Option<Wire>, headers: &http::HeaderMap, body: &Value) -> ScheduleRequest {
    match wire {
        Some(Wire::Codex) => crate::providers::chatgpt::api::scheduling(headers, body),
        Some(Wire::Xai) => crate::providers::grok::api::scheduling(headers, body),
        _ => {
            use crate::scheduler::{body_seed, hash, header_seed, text};
            let seed = header_seed(headers)
                .or_else(|| text(body, "prompt_cache_key"))
                .or_else(|| text(body, "previous_response_id"))
                .or_else(|| body_seed(body));
            ScheduleRequest {
                model: text(body, "model"),
                session_hash: seed.map(|seed| hash(&seed)),
                previous_response: text(body, "previous_response_id"),
                previous_can_move: false,
                ..Default::default()
            }
        }
    }
}

/// A channel error outside the executor as the failure a verdict judges.
pub fn send_failure_of(
    error: &UpstreamError,
    model: Option<&str>,
) -> crate::supply::UpstreamFailure {
    inspect::failure_of(error, model)
}
use serde_json::Value;
/// The downstream ledger's view of supplier attempts. Every attempt is attributed to
/// its supplier; recovered failures stay administrator-only.
pub trait AttemptLedger {
    fn rebind(
        &mut self,
        account: &SupplierAccount,
    ) -> impl std::future::Future<Output = Result<(), UpstreamError>> + Send;
    /// `recovery` is `retry` (same supplier) or `failover` (another supplier).
    fn attempt_failed(
        &mut self,
        error: &UpstreamError,
        recovery: &str,
    ) -> impl std::future::Future<Output = Result<(), UpstreamError>> + Send;
    fn retry_generation(&mut self);
    /// Capture actual output and usage of an attempt that cannot be replayed.
    fn measured_failure(&mut self, wire: Wire, failure: &UpstreamFailure);
}

/// A ledger that records nothing: model discovery and probes.
pub struct NoLedger;
impl AttemptLedger for NoLedger {
    async fn rebind(&mut self, _: &SupplierAccount) -> Result<(), UpstreamError> {
        Ok(())
    }
    async fn attempt_failed(&mut self, _: &UpstreamError, _: &str) -> Result<(), UpstreamError> {
        Ok(())
    }
    fn retry_generation(&mut self) {}
    fn measured_failure(&mut self, _: Wire, _: &UpstreamFailure) {}
}

/// A reply whose prelude passed inspection: output or a success body follows.
pub struct Executed {
    pub response: reqwest::Response,
    pub context: SupplierContext,
    pub wire: Wire,
    pub operation: Operation,
    pub stream: bool,
    pub model: Option<String>,
    pub auth_revision: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    /// No supplier in the pool can serve this request now.
    #[error("no available supplier")]
    NoSupplier {
        /// No supplier in the pool is configured for the model at all.
        model_not_found: bool,
    },
    /// Every eligible supplier is at its concurrency limit.
    #[error("every eligible supplier is busy")]
    Busy,
    /// A previous response can only continue on its original supplier, which is gone.
    #[error("the previous response cannot be continued")]
    ContextRequired,
    #[error("the consumer credential or route is no longer valid")]
    ConsumerInvalid,
    /// The supplier rejected the request itself; the body is in the wire's error shape.
    #[error("the supplier rejected the request")]
    Client {
        status: u16,
        body: Value,
        wire: Wire,
    },
    /// The channel stopped the generation; the body is in the wire's error shape.
    #[error("the supplier stopped the generation")]
    Stopped {
        status: u16,
        body: Value,
        wire: Wire,
    },
    #[error(transparent)]
    Convert(#[from] convert::Error),
    #[error(transparent)]
    Upstream(#[from] UpstreamError),
    #[error(transparent)]
    Storage(#[from] codex2api_storage::StorageError),
}

impl From<SchedulerError> for ExecutorError {
    fn from(error: SchedulerError) -> Self {
        match error {
            SchedulerError::Storage(error) => Self::Storage(error),
            SchedulerError::Upstream(error) => Self::Upstream(error),
            SchedulerError::ConsumerInvalid => Self::ConsumerInvalid,
            SchedulerError::Rejected { code, .. } => match code {
                Some("supplier_model_unavailable") => Self::NoSupplier {
                    model_not_found: true,
                },
                Some("supplier_concurrency_exceeded") => Self::Busy,
                Some("supplier_context_required") => Self::ContextRequired,
                _ => Self::NoSupplier {
                    model_not_found: false,
                },
            },
        }
    }
}

/// One attempt's failure: what the ledger records and what the channel judges.
pub(crate) struct AttemptFailure {
    pub error: UpstreamError,
    pub failure: UpstreamFailure,
}

/// Adjusts the request headers for the supplier an attempt is bound to.
pub type HeadersFor<'a> = Box<dyn Fn(&SupplierAccount, &mut http::HeaderMap) + Send + Sync + 'a>;

pub struct Execution<'a, L: AttemptLedger> {
    pub storage: &'a Storage,
    pub pool: &'a UpstreamPool,
    pub route: &'a SupplierRoute,
    pub schedule: ScheduleRequest,
    pub inbound: &'a InboundRequest,
    pub ledger: &'a mut L,
    /// Suppliers this request already gave up on.
    pub excluded: Vec<String>,
    /// Adjusts the request headers for the selected supplier, such as dropping a
    /// turn state another supplier minted.
    pub headers_for: Option<HeadersFor<'a>>,
    /// A supplier the listener already selected and reserved for the first attempt.
    pub initial: Option<SupplierContext>,
}

impl<'a, L: AttemptLedger> Execution<'a, L> {
    /// Run the request to a usable reply or a final error, switching suppliers as
    /// the channels' verdicts direct. `validate` re-checks the consumer before each
    /// selection and while waiting for capacity.
    pub async fn run<F, Fut>(mut self, validate: F) -> Result<Executed, ExecutorError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = scheduler::Result<()>>,
    {
        let mut attempts = Attempts::default();
        for id in &self.excluded {
            attempts.exclude(id);
        }
        'select: loop {
            validate().await?;
            let context = match self.initial.take() {
                Some(context) => context,
                None => {
                    scheduler::select(
                        self.storage,
                        self.route,
                        &self.schedule,
                        attempts.excluded(),
                        &validate,
                    )
                    .await?
                }
            };
            let account = context.account.clone();
            let wire = match self.pool.wire_of(&account).await {
                Some(wire)
                    if convert::supports(self.inbound.format, wire)
                        && self
                            .pool
                            .supports_operation(&account, self.inbound.operation) =>
                {
                    wire
                }
                _ => {
                    // The matrix has no conversion for this pair: not a switch, just
                    // never a candidate for this request.
                    attempts.exclude(&account.id);
                    continue;
                }
            };
            self.ledger.rebind(&account).await?;
            let mut retry = false;
            loop {
                if retry {
                    validate().await?;
                }
                retry = true;
                self.ledger.retry_generation();
                let mut request = convert::request(self.inbound, wire)?;
                if let Some(headers_for) = &self.headers_for {
                    headers_for(&account, &mut request.headers);
                    // The Codex header and its body metadata are the same capability.
                    convert::bind_turn_state(&mut request);
                }
                let stream = request.stream;
                let model = request.model.clone();
                let operation = request.operation;
                let revision = self.storage.supplier_auth_revision(&account.id).await?;
                let attempt = async {
                    let response = self
                        .pool
                        .send_at(&account, request, self.inbound.path.as_deref())
                        .await
                        .map_err(|error| inspect::send_failure(error, model.as_deref()))?;
                    inspect::inspect(wire, model.as_deref(), response).await
                }
                .await;
                match attempt {
                    Ok((response, observed_revision)) => {
                        return Ok(Executed {
                            response,
                            context,
                            wire,
                            operation,
                            stream,
                            model,
                            auth_revision: observed_revision.or(revision),
                        });
                    }
                    Err(AttemptFailure { error, failure }) => {
                        let revision = error.auth_revision().or(revision);
                        if failure.output_committed {
                            self.ledger.measured_failure(wire, &failure);
                        }
                        match attempts
                            .failed(self.pool, self.storage, &account, revision, wire, &failure)
                            .await?
                        {
                            Recovery::Retry(delay) => {
                                self.ledger.attempt_failed(&error, "retry").await?;
                                tokio::time::sleep(delay).await;
                            }
                            Recovery::Switch => {
                                self.ledger.attempt_failed(&error, "failover").await?;
                                drop(context);
                                continue 'select;
                            }
                        }
                    }
                }
            }
        }
    }
}

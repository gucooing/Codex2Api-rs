//! Channel dispatch: the one place shared code selects a channel by provider. Each
//! branch forwards to that channel's own transport and verdict.
use super::{AvoidScope, AvoidTarget, Operation, UpstreamFailure, Verdict, Wire, WireRequest};
use crate::{Result, UpstreamError, UpstreamPool};
use codex2api_storage::{BlockRequest, Storage, SupplierAccount};

impl UpstreamPool {
    /// The wire a supplier account speaks, or `None` for a provider without a channel
    /// or an API-key account whose credentials cannot be read.
    pub async fn wire_of(&self, account: &SupplierAccount) -> Option<Wire> {
        match account.provider_id.as_str() {
            codex2api_common::CHATGPT => Some(Wire::Codex),
            codex2api_common::GROK => Some(Wire::Xai),
            codex2api_common::ANTIGRAVITY => Some(Wire::Gemini),
            codex2api_common::API => self.api().wire(&account.id).await.ok(),
            _ => None,
        }
    }

    /// Channel operations beyond generation exist on the official subscription wires.
    pub fn supports_operation(&self, account: &SupplierAccount, operation: Operation) -> bool {
        match (account.provider_id.as_str(), operation) {
            (_, Operation::Generate) => codex2api_common::supported_provider(&account.provider_id),
            (codex2api_common::CHATGPT, _) => true,
            (codex2api_common::GROK, Operation::ImageGeneration | Operation::ImageEdit) => true,
            _ => false,
        }
    }

    /// Send one wire request on the account's isolated transport. `path` is a native
    /// client's own endpoint path under the supplier's root, when it has one.
    pub async fn send(
        &self,
        account: &SupplierAccount,
        request: WireRequest,
    ) -> Result<reqwest::Response> {
        self.send_at(account, request, None).await
    }

    pub async fn send_at(
        &self,
        account: &SupplierAccount,
        request: WireRequest,
        path: Option<&str>,
    ) -> Result<reqwest::Response> {
        match account.provider_id.as_str() {
            codex2api_common::CHATGPT => {
                crate::providers::chatgpt::api::send(self, &account.id, request, path).await
            }
            codex2api_common::GROK => {
                let path = match (request.operation, path) {
                    (Operation::Generate, Some(path)) => path,
                    (Operation::Generate, None) => "responses",
                    (Operation::ImageGeneration, _) => "images/generations",
                    (Operation::ImageEdit, _) => "images/edits",
                    _ => {
                        return Err(UpstreamError::Internal(
                            "Unsupported Grok upstream operation".into(),
                        ));
                    }
                };
                self.grok()
                    .grok_request_prepared(
                        &account.id,
                        http::Method::POST,
                        path,
                        Some(crate::grok::PreparedBody::new(request.body)?),
                        &request.headers,
                    )
                    .await
            }
            codex2api_common::ANTIGRAVITY => {
                self.antigravity().generate(&account.id, request.body).await
            }
            codex2api_common::API => self.api().send(&account.id, request).await,
            _ => Err(UpstreamError::Internal("Unsupported provider".into())),
        }
    }

    /// The channel's judgement of a failed attempt on this account.
    pub async fn verdict(&self, account: &SupplierAccount, failure: &UpstreamFailure) -> Verdict {
        match account.provider_id.as_str() {
            codex2api_common::CHATGPT => crate::providers::chatgpt::verdict::verdict(failure),
            codex2api_common::GROK => crate::providers::grok::verdict::verdict(failure),
            codex2api_common::ANTIGRAVITY => {
                crate::providers::antigravity::verdict::verdict(failure)
            }
            codex2api_common::API => {
                let retry = match self.api().credentials(&account.id).await {
                    Ok((credentials, _)) => credentials.retry_statuses.clone(),
                    Err(_) => Vec::new(),
                };
                crate::providers::api::verdict::verdict(failure, &retry)
            }
            _ => Verdict::avoid_request(),
        }
    }
}

/// Persist an `Avoid` verdict as a block. Request-scoped avoidance and other
/// verdicts persist nothing. Returns whether a block was written.
pub async fn apply_verdict(
    storage: &Storage,
    account_id: &str,
    auth_revision: Option<i64>,
    verdict: &Verdict,
) -> codex2api_storage::Result<bool> {
    let Verdict::Avoid {
        scope,
        target,
        kind,
        code,
        message,
    } = verdict
    else {
        return Ok(false);
    };
    let until_at = match scope {
        AvoidScope::Request => return Ok(false),
        AvoidScope::Until(until) => Some(*until),
        AvoidScope::Permanent => None,
    };
    let Some(revision) = auth_revision else {
        return Ok(false);
    };
    storage
        .block_supplier(
            account_id,
            revision,
            &BlockRequest {
                model: match target {
                    AvoidTarget::Account => None,
                    AvoidTarget::Model(model) => Some(model.clone()),
                },
                kind: *kind,
                until_at,
                code: code.clone(),
                message: message.clone(),
            },
        )
        .await
}

/// A failed channel reply outside the executor (probes, discovery, administrator
/// reads) judged and persisted by the same rules as execution.
pub async fn observe_error(
    pool: &UpstreamPool,
    storage: &Storage,
    account: &SupplierAccount,
    auth_revision: Option<i64>,
    error: &UpstreamError,
) -> codex2api_storage::Result<Verdict> {
    let failure = crate::executor::send_failure_of(error, None);
    let verdict = pool.verdict(account, &failure).await;
    apply_verdict(
        storage,
        &account.id,
        error.auth_revision().or(auth_revision),
        &verdict,
    )
    .await?;
    Ok(verdict)
}

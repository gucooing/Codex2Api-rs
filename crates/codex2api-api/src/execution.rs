//! Per-request execution orchestration; all transport paths share application authorization.
use crate::usage::RequestLog;
use codex2api_storage::{Storage, SupplierAccount, UsageRecord};
use codex2api_upstream::RequestMetadata;
use std::time::Instant;

#[derive(Clone)]
pub(crate) struct ExecutionContext {
    storage: Storage,
    provider_id: String,
    account_id: String,
    account_name: String,
    owner_id: String,
    owner_name: String,
    pub(crate) endpoint: String,
    transport: &'static str,
}

impl ExecutionContext {
    pub fn new(
        storage: Storage,
        account: &SupplierAccount,
        owner_id: &str,
        owner_name: &str,
        endpoint: &str,
        transport: &'static str,
    ) -> Self {
        Self {
            storage,
            provider_id: account.provider_id.clone(),
            account_id: account.id.clone(),
            account_name: account
                .display_name
                .as_deref()
                .filter(|s| !s.is_empty())
                .or(account.email.as_deref())
                .unwrap_or(&account.id)
                .into(),
            owner_id: owner_id.into(),
            owner_name: owner_name.into(),
            endpoint: endpoint.into(),
            transport,
        }
    }

    pub async fn authorize(
        &self,
        metadata: &RequestMetadata,
        inspect_only: bool,
    ) -> crate::Result<()> {
        use codex2api_core::ExecutionKind;
        let kind = if inspect_only {
            ExecutionKind::InspectModel
        } else if self.endpoint.ends_with("/responses/compact") {
            ExecutionKind::Compact
        } else if self.endpoint.ends_with("/responses") {
            ExecutionKind::Responses
        } else if self.endpoint.ends_with("/images/generations")
            || self.endpoint.ends_with("/images/edits")
        {
            ExecutionKind::Image
        } else {
            ExecutionKind::Unpriced
        };
        codex2api_service::ExecutionService::new(self.storage.clone())
            .authorize(
                &self.owner_id,
                &self.provider_id,
                codex2api_service::ExecutionRequest {
                    kind,
                    model: metadata.model.as_deref(),
                    service_tier: metadata.service_tier.as_deref(),
                    image_size: metadata.image_size.as_deref(),
                },
            )
            .await?;
        Ok(())
    }

    pub async fn start(
        &self,
        metadata: RequestMetadata,
        start: Instant,
        requested_at_ms: i64,
    ) -> crate::Result<RequestLog> {
        self.authorize(&metadata, false).await?;
        let record = UsageRecord {
            provider_id: self.provider_id.clone(),
            id: uuid::Uuid::new_v4().to_string(),
            account_id: self.account_id.clone(),
            account_name: self.account_name.clone(),
            subject_id: self.owner_id.clone(),
            subject_name: self.owner_name.clone(),
            endpoint: self.endpoint.clone(),
            transport: self.transport.into(),
            model: metadata.model,
            reasoning_effort: metadata.reasoning_effort,
            service_tier: metadata.service_tier,
            image_size: metadata.image_size,
            requested_at_ms,
            status: "in_progress".into(),
            ..Default::default()
        };
        RequestLog::begin(self.storage.clone(), record, start).await
    }
}

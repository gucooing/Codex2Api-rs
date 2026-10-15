//! Per-request execution orchestration; all transport paths share application authorization.
use crate::usage::RequestLog;
use codex2api_storage::{Storage, SupplierAccount, UsageRecord};
use codex2api_upstream::RequestMetadata;
use std::time::Instant;

pub(crate) struct Admission {
    metadata: RequestMetadata,
    pub(crate) version: (u64, u64, i64),
}
impl Admission {
    pub async fn new(
        storage: &Storage,
        provider: &str,
        owner: &str,
        endpoint: &str,
        metadata: RequestMetadata,
    ) -> crate::Result<Self> {
        codex2api_upstream::RequestFailureLog::metadata(&metadata);
        let version = storage.admission_version(owner);
        let service = codex2api_service::ExecutionService::new(storage.clone());
        service
            .authorize(
                owner,
                provider,
                request(provider, endpoint, &metadata, false),
            )
            .await?;
        service.admit_request(owner, false).await?;
        Ok(Self { metadata, version })
    }
}
fn request<'a>(
    provider: &str,
    endpoint: &str,
    metadata: &'a RequestMetadata,
    inspect: bool,
) -> codex2api_service::ExecutionRequest<'a> {
    use codex2api_core::ExecutionKind;
    let kind = if inspect {
        ExecutionKind::InspectModel
    } else if let Some(operation) = codex2api_core::billing_operation(provider, endpoint) {
        ExecutionKind::Operation(operation)
    } else if endpoint.ends_with("/responses/compact") {
        ExecutionKind::Compact
    } else if ["/responses", "/chat/completions", "/messages"]
        .iter()
        .any(|suffix| endpoint.ends_with(suffix))
    {
        ExecutionKind::Responses
    } else if ["/images/generations", "/images/edits"]
        .iter()
        .any(|suffix| endpoint.ends_with(suffix))
    {
        ExecutionKind::Image
    } else {
        ExecutionKind::Unpriced
    };
    codex2api_service::ExecutionRequest {
        kind,
        model: metadata.model.as_deref(),
        service_tier: metadata.service_tier.as_deref(),
        image_size: metadata.image_size.as_deref(),
    }
}

#[derive(Clone)]
pub(crate) struct ExecutionContext {
    supplier_permit: std::sync::Arc<std::sync::Mutex<Option<codex2api_storage::RuntimePermit>>>,
    storage: Storage,
    provider_id: String,
    account_id: String,
    account_name: String,
    owner_id: String,
    owner_name: String,
    pub(crate) endpoint: String,
    transport: &'static str,
    user_agent: Option<String>,
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
            supplier_permit: Default::default(),
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
            user_agent: None,
        }
    }

    pub fn with_supplier_permit(
        mut self,
        permit: Option<codex2api_storage::RuntimePermit>,
    ) -> Self {
        self.supplier_permit = std::sync::Arc::new(std::sync::Mutex::new(permit));
        self
    }
    pub fn take_supplier_permit(&self) -> Option<codex2api_storage::RuntimePermit> {
        self.supplier_permit.lock().unwrap().take()
    }

    pub fn with_user_agent(mut self, headers: &http::HeaderMap) -> Self {
        self.user_agent = codex2api_upstream::request_user_agent(headers);
        self
    }
    pub async fn authorize(
        &self,
        metadata: &RequestMetadata,
        inspect_only: bool,
    ) -> crate::Result<()> {
        codex2api_service::ExecutionService::new(self.storage.clone())
            .authorize(
                &self.owner_id,
                &self.provider_id,
                request(&self.provider_id, &self.endpoint, metadata, inspect_only),
            )
            .await?;
        if let Some(model) = metadata.model.as_deref()
            && !self
                .storage
                .supplier_supports_model(&self.account_id, model)
                .await?
        {
            return Err(crate::ApiError::openai(
                http::StatusCode::FORBIDDEN,
                "permission_error",
                "This supplier does not have the requested model configured.",
                Some("supplier_model_unavailable"),
            ));
        }
        Ok(())
    }

    pub async fn start(
        &self,
        metadata: RequestMetadata,
        start: Instant,
        requested_at_ms: i64,
    ) -> crate::Result<RequestLog> {
        let admission = Admission::new(
            &self.storage,
            &self.provider_id,
            &self.owner_id,
            &self.endpoint,
            metadata,
        )
        .await?;
        self.start_admitted(admission, start, requested_at_ms).await
    }

    pub async fn start_admitted(
        &self,
        admission: Admission,
        start: Instant,
        requested_at_ms: i64,
    ) -> crate::Result<RequestLog> {
        if admission.version != self.storage.admission_version(&self.owner_id) {
            self.authorize(&admission.metadata, false).await?;
        } else if let Some(model) = admission.metadata.model.as_deref()
            && !self
                .storage
                .supplier_supports_model(&self.account_id, model)
                .await?
        {
            return Err(crate::ApiError::openai(
                http::StatusCode::FORBIDDEN,
                "permission_error",
                "This supplier does not have the requested model configured.",
                Some("supplier_model_unavailable"),
            ));
        }
        let metadata = admission.metadata;
        let consumer_permit = self
            .storage
            .acquire_consumer(&self.owner_id)
            .await?
            .ok_or(codex2api_service::ServiceError::ConcurrencyExceeded)?;
        let supplier_permit = match self.take_supplier_permit() {
            Some(permit) => permit,
            None => {
                let policy = self.storage.supplier_scheduling(&self.account_id).await?;
                self.storage
                    .acquire_supplier(&self.account_id, policy.concurrency)
                    .await?
                    .ok_or_else(|| {
                        crate::ApiError::openai(
                            http::StatusCode::TOO_MANY_REQUESTS,
                            "rate_limit_error",
                            "The supplier is at its concurrency limit.",
                            Some("supplier_concurrency_exceeded"),
                        )
                    })?
            }
        };
        codex2api_service::ExecutionService::new(self.storage.clone())
            .admit_request(&self.owner_id, true)
            .await?;
        let record = UsageRecord {
            provider_id: self.provider_id.clone(),
            id: uuid::Uuid::new_v4().to_string(),
            account_id: self.account_id.clone(),
            account_name: self.account_name.clone(),
            subject_id: self.owner_id.clone(),
            subject_name: self.owner_name.clone(),
            endpoint: self.endpoint.clone(),
            transport: self.transport.into(),
            user_agent: self.user_agent.clone(),
            stream: Some(self.transport == "websocket" || metadata.stream.unwrap_or(false)),
            model: metadata.model,
            reasoning_effort: metadata.reasoning_effort,
            service_tier: metadata.service_tier,
            image_size: metadata.image_size,
            image_input_usage_json: metadata.image_input_sizes.map(|sizes| {
                serde_json::to_string(
                    &sizes
                        .into_iter()
                        .map(|resolution| codex2api_storage::ImageUsage {
                            resolution: Some(resolution),
                            count: 1,
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap_or_default()
            }),
            requested_at_ms,
            status: "in_progress".into(),
            ..Default::default()
        };
        let mut log = RequestLog::begin(self.storage.clone(), record, start).await?;
        log.set_supplier_permit(supplier_permit);
        log.set_consumer_permit(consumer_permit);
        Ok(log)
    }
}

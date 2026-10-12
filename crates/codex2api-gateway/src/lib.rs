//! API-key authentication, OpenAI protocol adaptation, per-user API limits and wallet billing.
//! Supplier scheduling and OAuth subscriptions remain independent application domains.
mod error;
mod execution;
mod images;
mod providers;
mod runtime;
mod usage;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Extension, Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
    routing::{get, post},
};
use codex2api_storage::{ApiPrincipal, Storage};
use codex2api_upstream::UpstreamPool;
pub use error::{GatewayError, Result};
pub(crate) use execution::execute_prepared;
pub use runtime::GatewayRuntime;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct GatewayState {
    pub(crate) storage: Storage,
    pub(crate) upstream: UpstreamPool,
    runtime: GatewayRuntime,
}
impl GatewayState {
    pub fn new(storage: Storage, upstream: UpstreamPool) -> Self {
        Self {
            storage,
            upstream,
            runtime: GatewayRuntime::default(),
        }
    }
    pub fn runtime(&self) -> &GatewayRuntime {
        &self.runtime
    }
}

#[derive(Clone)]
pub(crate) struct ApiCredential {
    pub(crate) principal: ApiPrincipal,
    pub(crate) token: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Protocol {
    Responses,
    ChatCompletions,
}
impl Protocol {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Responses => "/v1/responses",
            Self::ChatCompletions => "/v1/chat/completions",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputMode {
    Responses,
    ChatFromResponses,
    ChatCompletions,
    Images,
}
pub(crate) struct PreparedRequest {
    pub(crate) body: Value,
    pub(crate) endpoint: &'static str,
    pub(crate) upstream_path: &'static str,
    pub(crate) output: OutputMode,
    pub(crate) stream: bool,
    pub(crate) include_usage: bool,
}

pub fn router(state: GatewayState) -> Router {
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/responses", post(responses))
        .route("/v1/chat/completions", post(chat))
        .route("/v1/images/generations", post(images::generations))
        .route("/v1/images/edits", post(images::edits))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_key,
        ))
        .route("/healthz", get(|| async { Json(json!({"ok":true})) }))
        .layer(DefaultBodyLimit::max(codex2api_upstream::MAX_REQUEST_BYTES))
        .with_state(state)
}
async fn require_key(
    State(state): State<GatewayState>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let values = request
        .headers()
        .get_all(http::header::AUTHORIZATION)
        .iter()
        .collect::<Vec<_>>();
    if values.len() != 1 {
        return Err(GatewayError::invalid_key());
    }
    let (scheme, token) = values[0]
        .to_str()
        .ok()
        .and_then(|value| value.split_once(' '))
        .ok_or_else(GatewayError::invalid_key)?;
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || token.bytes().any(|b| b.is_ascii_whitespace())
    {
        return Err(GatewayError::invalid_key());
    }
    let token = token.to_owned();
    let principal = state
        .storage
        .authenticate_api_key(&token)
        .await?
        .ok_or_else(GatewayError::invalid_key)?;
    request
        .extensions_mut()
        .insert(ApiCredential { principal, token });
    Ok(next.run(request).await)
}
async fn models(
    State(state): State<GatewayState>,
    Extension(credential): Extension<ApiCredential>,
) -> Result<Json<Value>> {
    let models = state
        .storage
        .api_group_models(&credential.principal.group.id)
        .await?;
    let values=models.into_iter().map(|model|json!({"id":model.model,"object":"model","created":0,"owned_by":model.provider_id})).collect::<Vec<_>>();
    Ok(Json(json!({"object":"list","data":values})))
}
async fn responses(
    State(state): State<GatewayState>,
    Extension(credential): Extension<ApiCredential>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    handle(state, credential, Protocol::Responses, headers, body).await
}
async fn chat(
    State(state): State<GatewayState>,
    Extension(credential): Extension<ApiCredential>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    handle(state, credential, Protocol::ChatCompletions, headers, body).await
}
async fn handle(
    state: GatewayState,
    credential: ApiCredential,
    protocol: Protocol,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let inbound = headers.clone();
    let value =
        tokio::task::spawn_blocking(move || codex2api_upstream::decode_body(&body, &inbound))
            .await
            .map_err(|_| GatewayError::internal())??;
    let prepared = match credential.principal.group.provider_id.as_str() {
        codex2api_core::CHATGPT => providers::chatgpt::prepare(protocol, value)?,
        codex2api_core::GROK => providers::grok::prepare(protocol, value)?,
        _ => return Err(GatewayError::bad_request("Unsupported API group platform")),
    };
    execute_prepared(
        state,
        credential.principal,
        credential.token,
        headers,
        prepared,
    )
    .await
}
pub(crate) fn boolean(value: &Value, key: &str, default: bool) -> Result<bool> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(GatewayError::bad_request(format!(
            "{key} must be a boolean"
        ))),
    }
}
pub(crate) fn include_usage(value: &Value, stream: bool) -> Result<bool> {
    let Some(options) = value.get("stream_options").filter(|v| !v.is_null()) else {
        return Ok(false);
    };
    if !stream || !options.is_object() {
        return Err(GatewayError::bad_request(
            "stream_options requires stream=true and an object",
        ));
    }
    if options
        .as_object()
        .unwrap()
        .keys()
        .any(|key| key != "include_usage")
    {
        return Err(GatewayError::bad_request(
            "Unsupported stream_options parameter",
        ));
    }
    boolean(options, "include_usage", false)
}

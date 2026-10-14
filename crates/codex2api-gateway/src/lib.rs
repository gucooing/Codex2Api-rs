//! API-key authentication, OpenAI protocol adaptation, per-user API limits and wallet billing.
//! Supplier scheduling and OAuth subscriptions remain independent application domains.
mod error;
mod execution;
mod images;
mod providers;
mod request;
mod runtime;
mod usage;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Extension, Request, State, WebSocketUpgrade},
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
    Compact,
    InputTokens,
    Search,
}
impl Protocol {
    fn endpoint(self) -> &'static str {
        match self {
            Self::Responses => "/v1/responses",
            Self::ChatCompletions => "/v1/chat/completions",
            Self::Compact => "/v1/responses/compact",
            Self::InputTokens => "/v1/responses/input_tokens",
            Self::Search => "/v1/alpha/search",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputMode {
    Responses,
    ChatFromResponses,
    ChatCompletions,
    Images,
    Compact,
    InputTokens,
    Search,
}
pub(crate) struct PreparedRequest {
    pub(crate) body: Value,
    pub(crate) headers: HeaderMap,
    pub(crate) endpoint: &'static str,
    pub(crate) upstream_path: &'static str,
    pub(crate) output: OutputMode,
    pub(crate) stream: bool,
    pub(crate) include_usage: bool,
    pub(crate) legacy_functions: bool,
    pub(crate) metadata: Option<Value>,
}

pub fn router(state: GatewayState) -> Router {
    let mut router = Router::new()
        .route("/v1/models", get(models))
        .route("/v1/images/generations", post(images::generations))
        .route("/v1/images/edits", post(images::edits));
    for protocol in [
        Protocol::Responses,
        Protocol::ChatCompletions,
        Protocol::Compact,
        Protocol::InputTokens,
        Protocol::Search,
    ] {
        let route = post(handle).layer(Extension(protocol));
        let route = if protocol == Protocol::Responses {
            route.get(websocket)
        } else {
            route
        };
        router = router.route(protocol.endpoint(), route);
    }
    router
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_key,
        ))
        .route("/healthz", get(|| async { Json(json!({"ok":true})) }))
        .layer(DefaultBodyLimit::max(codex2api_upstream::MAX_REQUEST_BYTES))
        .with_state(state)
}

async fn websocket(
    State(state): State<GatewayState>,
    Extension(credential): Extension<ApiCredential>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    match credential.principal.group.provider_id.as_str() {
        codex2api_core::CHATGPT => {
            providers::chatgpt::websocket::connect(state, credential, headers, upgrade).await
        }
        _ => Err(GatewayError::new(
            http::StatusCode::NOT_IMPLEMENTED,
            "unsupported_transport",
            "This platform does not support Responses WebSocket transport.",
        )),
    }
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
async fn handle(
    State(state): State<GatewayState>,
    Extension(credential): Extension<ApiCredential>,
    Extension(protocol): Extension<Protocol>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let inbound = headers.clone();
    let value =
        tokio::task::spawn_blocking(move || codex2api_upstream::decode_body(&body, &inbound))
            .await
            .map_err(|_| GatewayError::internal())??;
    let prepared = match credential.principal.group.provider_id.as_str() {
        codex2api_core::CHATGPT => providers::chatgpt::prepare(protocol, value, &headers)?,
        codex2api_core::GROK => providers::grok::prepare(protocol, value, &headers)?,
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

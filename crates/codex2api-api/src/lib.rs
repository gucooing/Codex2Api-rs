//! API ingress, API-key admission, dispatch and wallet settlement.
//! Supplier request and response conversion belongs to codex2api-upstream.
//! Supplier scheduling and OAuth subscriptions remain independent application domains.
mod dispatch;
mod error;
mod execution;
mod images;
mod providers;
mod response;
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
pub use error::{ApiError, Result};
pub(crate) use execution::execute_prepared;
pub use runtime::ApiRuntime;
use serde_json::{Value, json};

#[derive(Clone)]
pub struct ApiState {
    pub(crate) storage: Storage,
    pub(crate) upstream: UpstreamPool,
    runtime: ApiRuntime,
}
impl ApiState {
    pub fn new(storage: Storage, upstream: UpstreamPool) -> Self {
        Self {
            runtime: ApiRuntime::new(storage.clone()),
            storage,
            upstream,
        }
    }
    pub fn runtime(&self) -> &ApiRuntime {
        &self.runtime
    }
}

#[derive(Clone)]
pub(crate) struct ApiCredential {
    pub(crate) principal: ApiPrincipal,
    pub(crate) token: String,
}

pub(crate) use codex2api_upstream::protocol::{InboundRequest, PreparedRequest, Protocol};

pub fn router(state: ApiState) -> Router {
    let mut router = Router::new()
        .route("/v1/models", get(models))
        .route("/v1/images/generations", post(images::generations))
        .route("/v1/images/edits", post(images::edits));
    for protocol in [
        Protocol::Responses,
        Protocol::ChatCompletions,
        Protocol::Messages,
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
        .layer(axum::middleware::from_fn(messages_errors))
        .layer(axum::middleware::from_fn(
            |request: Request, next: Next| async move {
                let peer = request
                    .extensions()
                    .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                    .map(|info| info.0);
                let client = codex2api_upstream::client_ip(peer, request.headers());
                codex2api_upstream::RequestClock::arrival(client)
                    .scope(next.run(request))
                    .await
            },
        ))
        .with_state(state)
}

async fn websocket(
    State(state): State<ApiState>,
    Extension(credential): Extension<ApiCredential>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response> {
    match credential.principal.group.provider_id.as_str() {
        codex2api_common::CHATGPT => {
            providers::chatgpt::websocket::connect(state, credential, headers, upgrade).await
        }
        _ => Err(ApiError::new(
            http::StatusCode::NOT_IMPLEMENTED,
            "unsupported_transport",
            "This platform does not support Responses WebSocket transport.",
        )),
    }
}

async fn require_key(
    State(state): State<ApiState>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let token =
        credential_token(request.headers(), request.uri().path() == "/v1/messages")?.to_owned();
    let principal = state
        .storage
        .authenticate_api_key(&token)
        .await?
        .ok_or_else(ApiError::invalid_key)?;
    let log = codex2api_upstream::RequestFailureLog::new(codex2api_storage::UsageRecord {
        provider_id: principal.group.provider_id.clone(),
        subject_id: principal.key_id.clone(),
        subject_kind: "api_key".into(),
        user_id: Some(principal.user_id.clone()),
        key_id: Some(principal.key_id.clone()),
        group_id: Some(principal.group.id.clone()),
        group_name: Some(principal.group.name.clone()),
        endpoint: request.uri().path().into(),
        transport: "http".into(),
        user_agent: codex2api_upstream::request_user_agent(request.headers()),
        ..Default::default()
    });
    let execution = request.uri().path() != "/v1/models";
    request
        .extensions_mut()
        .insert(ApiCredential { principal, token });
    let response = log.scope(next.run(request)).await;
    if !execution || !log.needs_record(response.status()) {
        return Ok(response);
    }
    let (parts, body) = response.into_parts();
    let body = log
        .capture(&state.storage, parts.status, body.into_data_stream())
        .await;
    Ok(Response::from_parts(
        parts,
        axum::body::Body::from_stream(body),
    ))
}
async fn models(
    State(state): State<ApiState>,
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
    State(state): State<ApiState>,
    Extension(credential): Extension<ApiCredential>,
    Extension(protocol): Extension<Protocol>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let inbound = headers.clone();
    let value =
        tokio::task::spawn_blocking(move || codex2api_upstream::decode_body(&body, &inbound))
            .await
            .map_err(|_| ApiError::internal())??;
    codex2api_upstream::RequestFailureLog::metadata(&codex2api_upstream::request_metadata_value(
        &value,
    ));
    // ChatGPT keeps stored-response continuation only on its native WebSocket transport.
    if credential.principal.group.provider_id == codex2api_common::CHATGPT
        && protocol == Protocol::Responses
        && value
            .get("previous_response_id")
            .is_some_and(|v| !v.is_null())
    {
        return Err(ApiError::bad_request(
            "previous_response_id requires the Responses WebSocket transport for ChatGPT",
        ));
    }
    let prepared = InboundRequest::new(protocol, value, headers.clone())?;
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
        _ => Err(ApiError::bad_request(format!("{key} must be a boolean"))),
    }
}

fn credential_token(headers: &HeaderMap, messages: bool) -> Result<&str> {
    let authorization = headers
        .get_all(http::header::AUTHORIZATION)
        .iter()
        .collect::<Vec<_>>();
    let keys = headers.get_all("x-api-key").iter().collect::<Vec<_>>();
    let token = if !authorization.is_empty() {
        if authorization.len() != 1 {
            return Err(ApiError::invalid_key());
        }
        let (scheme, token) = authorization[0]
            .to_str()
            .ok()
            .and_then(|v| v.split_once(' '))
            .ok_or_else(ApiError::invalid_key)?;
        if !scheme.eq_ignore_ascii_case("bearer") {
            return Err(ApiError::invalid_key());
        }
        if messages
            && (!keys.is_empty() && (keys.len() != 1 || keys[0].to_str().ok() != Some(token)))
        {
            return Err(ApiError::invalid_key());
        }
        token
    } else if messages && keys.len() == 1 {
        keys[0].to_str().map_err(|_| ApiError::invalid_key())?
    } else {
        return Err(ApiError::invalid_key());
    };
    if token.is_empty() || token.bytes().any(|b| b.is_ascii_whitespace()) {
        return Err(ApiError::invalid_key());
    }
    Ok(token)
}
async fn messages_errors(request: Request, next: Next) -> Response {
    let messages = request.uri().path() == "/v1/messages";
    let response = next.run(request).await;
    if !messages || response.status().is_success() {
        return response;
    }
    let (parts, body) = response.into_parts();
    let value = axum::body::to_bytes(body, codex2api_upstream::MAX_REQUEST_BYTES)
        .await
        .ok()
        .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
        .unwrap_or_else(|| {
            ApiError::new(
                parts.status,
                "invalid_request_error",
                "The request could not be completed.",
            )
            .body
        });
    let value = codex2api_upstream::protocol::messages::error(parts.status, &value);
    let mut response = Response::from_parts(parts, axum::body::Body::from(value.to_string()));
    response.headers_mut().remove(http::header::CONTENT_LENGTH);
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    response
}

//! Grok Build client protocol; all identity, settings and budgets are local.
mod jwt;
mod oauth;
mod usage;
use crate::{ApiError, ApiState, Result};
use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Extension, Request, State},
    http::{HeaderMap, Method, StatusCode},
    middleware::Next,
    response::Response,
    routing::{get, post},
};
use codex2api_storage::{VirtualAccess, hash_token};
use serde_json::{Value, json};

pub fn router(state: ApiState) -> Router {
    let private = Router::new()
        .route(
            "/v1/responses",
            post(responses).layer(Extension("responses")),
        )
        .route(
            "/v1/chat/completions",
            post(responses).layer(Extension("chat/completions")),
        )
        .route("/v1/messages", post(responses).layer(Extension("messages")))
        .route("/v1/models", get(models))
        .route("/v1/user", get(user))
        .route("/oauth2/userinfo", get(userinfo))
        .route("/v1/settings", get(settings))
        .route("/v1/billing", get(billing))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_oauth,
        ));
    let routes = private
        .route("/.well-known/openid-configuration", get(oauth::discovery))
        .route("/.well-known/jwks.json", get(oauth::jwks))
        .route("/oauth2/authorize", get(oauth::authorize))
        .route("/oauth2/token", post(oauth::token))
        .route("/oauth2/revoke", post(oauth::revoke))
        .route("/oauth2/device/code", post(oauth::device))
        .route(
            "/v1/login-config",
            get(|| async { Json(json!({"device_flow":false})) }),
        );

    Router::new()
        .nest("/grok", routes.clone())
        .nest("/api/oauth/grok", routes)
        .layer(DefaultBodyLimit::max(codex2api_upstream::MAX_REQUEST_BYTES))
        .with_state(state)
}

pub async fn require_oauth(
    State(state): State<ApiState>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let values = request
        .headers()
        .get_all(http::header::AUTHORIZATION)
        .iter()
        .collect::<Vec<_>>();
    if values.len() != 1 {
        return Err(ApiError::invalid_token());
    }
    let token = values[0]
        .to_str()
        .ok()
        .and_then(|s| {
            s.strip_prefix("Bearer ")
                .or_else(|| s.strip_prefix("bearer "))
        })
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(ApiError::invalid_token)?;
    let access = state
        .storage
        .virtual_access(&hash_token(token))
        .await?
        .ok_or_else(ApiError::invalid_token)?;
    if access.provider_id != codex2api_core::GROK {
        return Err(codex2api_service::ServiceError::Policy(
            codex2api_core::PolicyError::ProviderMismatch,
        )
        .into());
    }
    for name in ["x-userid", "x-grok-user-id"] {
        if request
            .headers()
            .get_all(name)
            .iter()
            .any(|v| v.to_str().ok() != Some(access.virtual_account_id.as_str()))
        {
            return Err(ApiError::openai(
                StatusCode::FORBIDDEN,
                "permission_error",
                "Account mismatch.",
                Some("account_mismatch"),
            ));
        }
    }
    if request.method() == Method::POST
        && !access
            .scopes
            .split_whitespace()
            .any(|s| matches!(s, "api:access" | "grok-cli:access"))
    {
        return Err(ApiError::openai(
            StatusCode::FORBIDDEN,
            "permission_error",
            "Required OAuth scope is missing.",
            Some("insufficient_scope"),
        ));
    }
    state
        .storage
        .touch_virtual_access(&access.token_hash)
        .await?;
    let method = request.method().to_string();
    let path = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|p| p.as_str())
        .unwrap_or(request.uri().path())
        .to_owned();
    let start = std::time::Instant::now();
    request.extensions_mut().insert(access.clone());
    let response = next.run(request).await;
    if let Err(error) = state
        .storage
        .record_virtual_request(
            &access.virtual_account_id,
            &access.device_id,
            &method,
            &path,
            response.status().as_u16(),
            start.elapsed().as_millis().min(i64::MAX as u128) as i64,
        )
        .await
    {
        tracing::warn!(%error,"Failed to record Grok client request");
    }
    Ok(response)
}

pub(crate) async fn catalog(state: &ApiState, owner: &str) -> Result<Value> {
    let models = state.storage.grok_consumer_models(owner).await?;
    Ok(json!({"object":"list","data":models}))
}
async fn models(
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> Result<Json<Value>> {
    Ok(Json(catalog(&state, &access.virtual_account_id).await?))
}
async fn account(
    state: &ApiState,
    access: &VirtualAccess,
) -> Result<codex2api_storage::VirtualAccount> {
    state
        .storage
        .effective_virtual_account(&access.virtual_account_id)
        .await?
        .filter(|a| a.enabled && a.provider_id == codex2api_core::GROK)
        .ok_or_else(ApiError::invalid_token)
}
async fn user(
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> Result<Json<Value>> {
    let account = account(&state, &access).await?;
    let entitlements = state.storage.effective_entitlements(&account.id).await?;
    let tier = codex2api_core::providers::grok::subscription(account.effective_plan());
    let user_tier = tier.map_or(account.effective_plan(), |t| t.user_tier);
    let mut value = json!({"userId":account.id,"principalType":"User","principalId":account.id,"subscriptionTier":user_tier,"hasGrokCodeAccess":entitlements.execution_enabled});
    if access.scopes.split_whitespace().any(|s| s == "email") {
        value["email"] = account.email.into();
    }
    if access.scopes.split_whitespace().any(|s| s == "profile") {
        value["firstName"] = account.name.into();
    }
    Ok(Json(value))
}
async fn userinfo(
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> Result<Json<Value>> {
    let account = account(&state, &access).await?;
    let mut value = json!({"sub":account.id});
    if access.scopes.split_whitespace().any(|s| s == "email") {
        value["email"] = account.email.into();
        value["email_verified"] = false.into();
    }
    if access.scopes.split_whitespace().any(|s| s == "profile") {
        value["name"] = account.name.into();
    }
    Ok(Json(value))
}
async fn settings(
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> Result<Json<Value>> {
    let account = account(&state, &access).await?;
    let entitlements = state.storage.effective_entitlements(&account.id).await?;
    let tier = codex2api_core::providers::grok::subscription(account.effective_plan());
    let (id, display) = tier.map_or((account.effective_plan(), account.effective_plan()), |t| {
        (t.id, t.display)
    });
    Ok(Json(
        json!({"allow_access":entitlements.execution_enabled,"subscription_tier":id,"subscription_tier_display":display,"on_demand_enabled":false}),
    ))
}
async fn billing(
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> Result<Json<Value>> {
    let account = account(&state, &access).await?;
    let quota = state.storage.virtual_quota(&account.id).await?;
    let window = quota["rate_limit"]["windows"].as_array().and_then(|w| {
        w.iter()
            .max_by_key(|w| w["used_percent"].as_i64().unwrap_or(0))
    });
    let date = |v: &Value| {
        v.as_i64()
            .and_then(|n| chrono::DateTime::from_timestamp(n, 0))
            .map(|t| t.to_rfc3339())
    };
    let config=window.map(|w|json!({"creditUsagePercent":w["used_percent"],"currentPeriod":{"start":date(&w["started_at"]),"end":date(&w["reset_at"])}}));
    let tier = codex2api_core::providers::grok::subscription(account.effective_plan());
    Ok(Json(
        json!({"config":config,"on_demand_enabled":false,"subscription_tier":tier.map_or(account.effective_plan(), |t| t.id)}),
    ))
}
async fn responses(
    _: crate::user_agent::AllowedUserAgent,
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Extension(endpoint): Extension<&'static str>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    forward(state, access, headers, body, endpoint).await
}
pub(crate) async fn forward(
    state: ApiState,
    access: VirtualAccess,
    headers: HeaderMap,
    body: Bytes,
    endpoint: &'static str,
) -> Result<Response> {
    crate::response::complete_on_disconnect(async move {
        let mut body = codex2api_upstream::grok::prepare(&body, &headers)?;
        if endpoint == "messages" {
            body.as_object_mut()
                .expect("validated object")
                .remove("store");
        }
        if endpoint == "chat/completions" && body["stream"] == true {
            body["stream_options"] = json!({"include_usage":true});
        }
        let metadata = codex2api_upstream::request_metadata(
            &codex2api_upstream::grok::encoded(&body)?,
            &HeaderMap::new(),
        )?;
        let ctx = crate::pool_execution::select(&state, &access, &[]).await?;
        let mut log = Some(
            crate::execution::ExecutionContext::new(
                state.storage.clone(),
                &ctx.account,
                &access.virtual_account_id,
                &access.name,
                &format!("/grok/v1/{endpoint}"),
                "http",
            )
            .start(
                metadata,
                std::time::Instant::now(),
                chrono::Utc::now().timestamp_millis(),
            )
            .await?,
        );
        let response = crate::pool_execution::execute_with_prelude(
            &state,
            &access,
            ctx,
            &mut log,
            usage::prelude,
            |id| {
                let body = body.clone();
                let headers = headers.clone();
                let upstream = state.upstream.clone();
                async move {
                    upstream
                        .grok()
                        .grok_request(&id, Method::POST, endpoint, Some(body), &headers)
                        .await
                }
            },
        )
        .await?;
        let status = response.status();
        let response_headers = response.headers().clone();
        log.as_mut()
            .expect("request ledger")
            .set_protocol_observer(usage::Observer::default());
        let sse = response_headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        let body = log
            .take()
            .expect("request ledger")
            .wrap_with(response, |body| if sse { isolate(body) } else { body });
        Ok(crate::response::forward_response(
            status,
            response_headers,
            body,
        ))
    })
    .await
}
fn isolate(body: Body) -> Body {
    use eventsource_stream::Eventsource;
    use futures::StreamExt;
    Body::from_stream(
        body.into_data_stream()
            .eventsource()
            .filter_map(|event| async move {
                let event = match event {
                    Ok(v) => v,
                    Err(e) => return Some(Err(std::io::Error::other(e.to_string()))),
                };
                if event.data.trim() == "[DONE]" {
                    return Some(Ok(Bytes::from_static(b"data: [DONE]\n\n")));
                }
                let mut value: Value = match serde_json::from_str(&event.data) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(std::io::Error::other(e))),
                };
                if value["type"]
                    .as_str()
                    .is_some_and(|s| s.contains("rate_limit") || s.contains("billing"))
                {
                    return None;
                }
                if value.get("error").is_some_and(|v| !v.is_null()) {
                    value["error"] = codex2api_upstream::grok::errors::normalize(&value);
                    if value["type"].is_null() {
                        value["type"] = "error".into();
                    }
                }
                crate::public_output::metadata(&mut value);
                Some(Ok(Bytes::from(format!(
                    "event: {}\ndata: {}\n\n",
                    event.event, value
                ))))
            }),
    )
}

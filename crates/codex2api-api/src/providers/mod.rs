//! Composition root. Provider routers never call each other's protocol handlers.
pub(crate) mod chatgpt;
pub(crate) mod grok;

pub(crate) fn router(state: crate::ApiState) -> axum::Router {
    use tower::ServiceExt;
    let grok = grok::router(state.clone());
    let storage = state.storage.clone();
    let dispatch = grok.clone();
    chatgpt::router(state)
        .merge(grok)
        .layer(axum::middleware::from_fn(
            move |mut request: axum::extract::Request, next: axum::middleware::Next| {
                let grok = dispatch.clone();
                let storage = storage.clone();
                async move {
                    let path = request.uri().path();
                    let shared = (path == "/v1/responses"
                        && request.method() == http::Method::POST)
                        || (path == "/v1/models" && request.method() == http::Method::GET);
                    if shared {
                        let token = request
                            .headers()
                            .get(http::header::AUTHORIZATION)
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| {
                                v.strip_prefix("Bearer ")
                                    .or_else(|| v.strip_prefix("bearer "))
                            });
                        if let Some(token) = token {
                            match storage
                                .virtual_access(&codex2api_storage::hash_token(token.trim()))
                                .await
                            {
                                Ok(Some(access)) if access.provider_id == codex2api_core::GROK => {
                                    let uri = format!("/grok{}", request.uri());
                                    *request.uri_mut() =
                                        uri.parse().expect("known provider prefix");
                                    return grok.oneshot(request).await.expect("infallible router");
                                }
                                Err(error) => {
                                    return axum::response::IntoResponse::into_response(
                                        crate::ApiError::from(error),
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                    next.run(request).await
                }
            },
        ))
}

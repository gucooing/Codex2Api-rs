//! Composition root. Each provider owns one explicitly scoped client protocol.
pub(crate) mod chatgpt;
pub(crate) mod grok;

pub(crate) fn router(state: crate::OAuthState) -> axum::Router {
    chatgpt::router(state.clone())
        .merge(grok::router(state))
        .layer(axum::middleware::from_fn(
            |request: axum::extract::Request, next: axum::middleware::Next| async move {
                codex2api_upstream::RequestClock::default()
                    .scope(next.run(request))
                    .await
            },
        ))
}

//! Composition root. Each provider owns one explicitly scoped client protocol.
pub(crate) mod chatgpt;
pub(crate) mod grok;

pub(crate) fn router(state: crate::OAuthState) -> axum::Router {
    chatgpt::router(state.clone())
        .merge(grok::router(state))
        .layer(axum::middleware::from_fn(
            |request: axum::extract::Request, next: axum::middleware::Next| async move {
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
}

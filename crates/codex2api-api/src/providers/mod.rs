//! Installed protocol adapters. A provider is exposed only after its routes,
//! authentication and contracts have been implemented and validated.
pub(crate) mod chatgpt;

pub(crate) fn router(state: crate::ApiState) -> axum::Router {
    chatgpt::router(state)
}

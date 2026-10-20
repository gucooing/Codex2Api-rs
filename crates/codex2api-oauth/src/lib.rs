//! Configured-client OAuth surface for ccodex, cgrok and hooked Desktop.
//! Provider protocols are scoped under /v1/{platform}; subscription admission
//! uses the local admission module. Standard API-key inference belongs to codex2api-api.
mod error;
mod execution;
mod pool_execution;
mod pool_scheduler;
mod providers;
mod response;
mod state;
mod usage;
mod user_agent;

pub use error::{OAuthError, OpenAiError, OpenAiErrorBody, Result, openai_json};
pub use state::OAuthState;

pub fn router(state: OAuthState) -> axum::Router {
    providers::router(state)
}

pub mod admission;

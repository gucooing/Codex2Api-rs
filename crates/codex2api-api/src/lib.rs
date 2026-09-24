//! Virtual-account HTTP surface. Provider contracts are isolated in providers/;
//! execution authorization is shared through codex2api-service.
mod error;
mod execution;
mod providers;
mod response;
mod state;
mod usage;
mod user_agent;

pub use error::{ApiError, OpenAiError, OpenAiErrorBody, Result, openai_json};
pub use state::ApiState;

pub fn router(state: ApiState) -> axum::Router {
    providers::router(state)
}

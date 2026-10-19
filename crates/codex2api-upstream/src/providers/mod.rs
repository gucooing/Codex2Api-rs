//! Provider adapters. Shared code dispatches by provider here and nowhere else.
pub mod antigravity;
pub mod chatgpt;
pub mod grok;
use serde_json::Value;

/// Events a supplier may emit before any output; the scheduler buffers them so a
/// rejected attempt can still move to another account.
pub fn prelude(provider: &str) -> fn(&Value) -> bool {
    match provider {
        codex2api_common::GROK => grok::prelude,
        _ => crate::scheduler::prelude_event,
    }
}

/// A supplier error envelope in the shared shape, before accounting and public filtering.
/// Antigravity normalizes errors in its transport; ChatGPT already speaks the shared shape.
pub fn normalize_event(provider: &str, value: &mut Value) {
    if provider == codex2api_common::GROK {
        grok::errors::normalize_response(value);
    }
}

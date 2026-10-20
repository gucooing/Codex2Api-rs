//! Supplier channels. Each owns its transport, identity binding and verdicts;
//! shared code dispatches by provider in `supply::pool` and nowhere else.
pub mod antigravity;
pub mod api;
pub mod chatgpt;
pub mod grok;

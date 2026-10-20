//! The Anthropic Messages wire reached with a key. Requests are built directly from
//! each downstream format; Messages events are converted into the caller's protocol.
pub mod events;
pub mod request;

pub use request::build;

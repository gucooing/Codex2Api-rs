//! The Cloud Code Assist Gemini wire (Antigravity). Requests are built directly from
//! each downstream format; the desktop client's `v1internal` envelope is added by the
//! channel. Gemini SSE chunks are converted into the caller's protocol events.
pub mod events;
pub(crate) mod images;
pub mod request;
pub(crate) mod schema;

pub use request::build;

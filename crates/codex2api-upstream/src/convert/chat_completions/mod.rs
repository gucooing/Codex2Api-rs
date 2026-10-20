//! The Chat Completions wire (the OpenAI API and compatible services reached with a
//! key). Requests are built directly from each downstream format; chunks are
//! converted into the caller's protocol.
pub mod events;
pub mod request;

pub use request::build;

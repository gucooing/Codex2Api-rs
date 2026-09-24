//! ChatGPT consumer protocol and supplier transport integration.
pub(crate) mod access;
pub(crate) mod handlers;
pub(crate) mod identity;
mod routes;
pub(crate) use routes::router;

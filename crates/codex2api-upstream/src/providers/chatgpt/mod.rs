//! ChatGPT upstream wire protocol only.
pub(crate) mod backend;
pub(crate) mod catalog;
pub(crate) mod client;
pub(crate) mod endpoint;
pub(crate) mod headers;
pub(crate) mod operations;
pub(crate) mod proxy;
pub(crate) mod quota;
pub(crate) mod request;
pub(crate) mod routing;
pub(crate) mod timezone;
pub(crate) mod websocket;

mod model_discovery;

pub(crate) mod api;
pub(crate) mod api_images;
pub(crate) mod verdict;

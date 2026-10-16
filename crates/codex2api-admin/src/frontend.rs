//! The admin website embedded in the Rust service.
use axum::{
    Router,
    extract::OriginalUri,
    response::{Redirect, Response},
    routing::get,
};
include!(concat!(env!("OUT_DIR"), "/ASSETS.rs"));

pub fn router() -> Router {
    Router::new()
        .route("/", get(|| async { Redirect::to("/admin/") }))
        .route("/admin", get(|| async { Redirect::permanent("/admin/") }))
        .route("/admin/", get(serve))
        .route("/admin/{*path}", get(serve))
}
async fn serve(OriginalUri(uri): OriginalUri) -> Response {
    codex2api_common::frontend::serve_assets(uri, "/admin/", ASSETS)
}

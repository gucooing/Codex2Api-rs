use axum::Json;
use serde_json::{Value, json};

pub(crate) async fn healthz() -> Json<Value> {
    Json(json!({ "ok": true }))
}

pub(crate) async fn version() -> Json<Value> {
    Json(
        serde_json::to_value(codex2api_version::reference()).unwrap_or(json!({
            "commit": codex2api_version::CODEX_REF_COMMIT,
        })),
    )
}

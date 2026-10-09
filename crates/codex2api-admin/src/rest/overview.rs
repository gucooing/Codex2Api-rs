use crate::{AdminState, rest::error::ApiResult};
use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::Deserialize;

pub(super) fn router() -> Router<AdminState> {
    Router::new()
        .route("/overview", get(overview))
        .route("/overview/usage", get(super::usage::statistics))
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct OverviewQuery {
    tz_offset: i32,
}

pub async fn overview(
    State(s): State<AdminState>,
    Query(query): Query<OverviewQuery>,
) -> ApiResult {
    Ok(Json(s.storage.admin_overview(query.tz_offset).await?))
}

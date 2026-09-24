use axum::{
    Json,
    extract::{Extension, OriginalUri, State},
    response::Response,
};
use codex2api_storage::{FamilyGraduationNotice, VirtualAccess};
use serde::Deserialize;
use serde_json::json;

fn account_query(access: &VirtualAccess, uri: &axum::http::Uri) -> crate::Result<()> {
    for (key, value) in url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()) {
        if key == "account_id" {
            super::virtual_data::account_match(access, &value)?;
        }
    }
    Ok(())
}

pub(crate) async fn read(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    OriginalUri(uri): OriginalUri,
) -> crate::Result<Response> {
    account_query(&access, &uri)?;
    let notices = state
        .storage
        .family_graduation_notices(&access.virtual_account_id, false)
        .await?;
    Ok(crate::providers::chatgpt::identity::json_response(
        json!({"notices":notices.iter().map(FamilyGraduationNotice::client_value).collect::<Vec<_>>()}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Dismiss {
    notice_ids: Vec<String>,
}

pub(crate) async fn dismiss(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    OriginalUri(uri): OriginalUri,
    Json(input): Json<Dismiss>,
) -> crate::Result<Response> {
    account_query(&access, &uri)?;
    if input.notice_ids.is_empty()
        || input.notice_ids.len() > 1000
        || input
            .notice_ids
            .iter()
            .any(|id| id.is_empty() || id.len() > 256)
    {
        return Err(crate::ApiError::bad_request("Expected 1–1000 notice IDs."));
    }
    if !state
        .storage
        .dismiss_family_graduation_notices(&access, &input.notice_ids)
        .await?
    {
        return Err(super::virtual_data::not_found());
    }
    Ok(crate::providers::chatgpt::identity::json_response(
        json!({}),
    ))
}

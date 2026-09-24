use crate::{ApiError, ApiState};
use axum::{
    extract::{OriginalUri, State},
    http::Method,
    response::{IntoResponse, Response},
};

pub async fn endpoint(
    State(state): State<ApiState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
) -> Response {
    // Only path structure is retained; never query strings, headers or bodies.
    let path = uri
        .path()
        .split('/')
        .map(|part| {
            if part.len() > 96
                || part.starts_with("c2rt_")
                || part.starts_with("eyJ")
                || part.contains('@')
            {
                ":redacted"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("/")
        .chars()
        .take(1024)
        .collect::<String>();
    if let Err(err) = state
        .storage
        .record_missing_endpoint(method.as_str(), &path)
        .await
    {
        tracing::error!(%err,"could not record unimplemented endpoint");
    }
    tracing::warn!(method=%method,path=%path,"unimplemented OAuth client endpoint");
    ApiError::openai(axum::http::StatusCode::NOT_IMPLEMENTED,"api_error","This client endpoint has not been implemented. The method and path were recorded in OAuth administration.",Some("endpoint_not_implemented")).into_response()
}

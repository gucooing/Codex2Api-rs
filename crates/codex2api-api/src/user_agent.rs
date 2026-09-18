use axum::extract::FromRequestParts;
use axum::http::{StatusCode, header::USER_AGENT, request::Parts};
use codex2api_upstream::Endpoint;

use crate::{ApiError, ApiState};

/// Reject billable requests before reading bodies, upgrading sockets or using upstream accounts.
pub(crate) struct AllowedUserAgent;

impl FromRequestParts<ApiState> for AllowedUserAgent {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &ApiState) -> Result<Self, ApiError> {
        if parts
            .extensions
            .get::<Endpoint>()
            .is_some_and(|endpoint| !crate::usage::billable(*endpoint))
        {
            return Ok(Self);
        }
        let user_agent = parts
            .headers
            .get_all(USER_AGENT)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect::<Vec<_>>()
            .join(" ");
        if !state
            .storage
            .gateway_settings()
            .await?
            .allows_user_agent(&user_agent)
        {
            return Err(ApiError::openai(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_error",
                "Request rejected by the gateway User-Agent policy.",
                Some("user_agent_blocked"),
            ));
        }
        Ok(Self)
    }
}

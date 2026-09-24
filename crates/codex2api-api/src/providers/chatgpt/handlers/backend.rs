use crate::{ApiState, Result};
use axum::body::Bytes;
use axum::extract::{Extension, OriginalUri, Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use codex2api_upstream::BackendEndpoint;
use std::collections::HashMap;

pub async fn forward(
    State(state): State<ApiState>,
    Extension(endpoint): Extension<BackendEndpoint>,
    Extension(access): Extension<codex2api_storage::VirtualAccess>,
    Path(parameters): Path<HashMap<String, String>>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let account = state
        .storage
        .virtual_account(&access.virtual_account_id)
        .await?
        .ok_or_else(crate::ApiError::invalid_token)?;
    for (key, value) in url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()) {
        if key == "account_id" && value != account.id {
            return Err(crate::ApiError::bad_request("SupplierAccount mismatch."));
        }
    }
    let local = match endpoint {
        BackendEndpoint::Accounts => {
            let mut value = crate::providers::chatgpt::identity::workspace_check(&account);
            // Actual Desktop net.fetch uses Chromium's UA; some request profiles
            // use its surface UA. Native OAuth uses the CLI UA with an OS version.
            let desktop = headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|ua| {
                    (ua.starts_with("Mozilla/") && ua.contains("Chrome/"))
                        || (ua.starts_with("Codex Desktop/")
                            && ua.split_once(" (").is_some_and(|(_, tail)| {
                                ["Windows; ", "macOS; ", "Linux; "]
                                    .iter()
                                    .any(|os| tail.starts_with(os))
                            }))
                });
            if desktop {
                value["accounts"][0]["workspace_backend_origin"] = "NO_CONSTRAINT".into();
            }
            Some(value)
        }
        BackendEndpoint::Profile => {
            let stats = state.storage.virtual_usage_summary(&account.id).await?;
            let display = state
                .storage
                .virtual_config(&account.id, "profile_page")
                .await?
                .value;
            let private = state
                .storage
                .virtual_config(&account.id, "profile")
                .await?
                .value;
            Some(serde_json::json!({
                "profile":{
                    "id":account.id,"display_name":display["display_name"].as_str().filter(|s|!s.trim().is_empty()).unwrap_or(&account.name),"username":display["username"].as_str().filter(|s|!s.trim().is_empty()).unwrap_or(&account.username),
                    "profile_picture_url":private["picture"].as_str().map(str::trim)
                },
                "stats":stats,
                "metadata":{"stats_error":null}
            }))
        }
        // This is the virtual workspace's configuration, not the supplier account's policy.
        BackendEndpoint::Config => Some(
            state
                .storage
                .virtual_config(&account.id, "config_bundle")
                .await?
                .value,
        ),
        BackendEndpoint::Settings => Some(
            state
                .storage
                .virtual_config(&account.id, "cloud_preferences")
                .await?
                .value
                .clone(),
        ),
        BackendEndpoint::Messages => Some(
            state
                .storage
                .virtual_config(&account.id, "workspace_messages")
                .await?
                .value,
        ),
        BackendEndpoint::Usage => {
            let mut quota = state.storage.virtual_quota(&account.id).await?;
            // The client consumes protocol windows; billing amounts belong in admin views.
            quota.as_object_mut().unwrap().remove("billing");
            // The installed Desktop reader only accepts the official primary and
            // secondary fields; the full nested summary stays server-side.
            quota.as_object_mut().unwrap().remove("windows");
            for key in ["primary_window", "secondary_window"] {
                if let Some(window) = quota["rate_limit"][key].as_object_mut() {
                    window.retain(|key, _| {
                        matches!(
                            key.as_str(),
                            "used_percent"
                                | "limit_window_seconds"
                                | "reset_at"
                                | "reset_after_seconds"
                        )
                    });
                }
            }
            Some(quota)
        }
        BackendEndpoint::Credits => {
            let credits = state
                .storage
                .virtual_resources(&account.id, "reset_credit")
                .await?;
            let count = credits
                .iter()
                .filter(|v| v["status"] == "available")
                .count();
            Some(serde_json::json!({"credits":credits,"available_count":count}))
        }
        BackendEndpoint::ConsumeCredit => {
            return Err(crate::ApiError::bad_request(
                "Virtual Token quotas do not use supplier reset credits.",
            ));
        }
        BackendEndpoint::Tasks
        | BackendEndpoint::Task
        | BackendEndpoint::SiblingTurns
        | BackendEndpoint::TaskTurns
        | BackendEndpoint::TaskTurn
        | BackendEndpoint::TaskLogs
        | BackendEndpoint::CancelTask
        | BackendEndpoint::ArchiveTask
        | BackendEndpoint::CreateTask => {
            return super::virtual_tasks::forward(
                &state,
                &account,
                access.clone(),
                super::virtual_tasks::TaskRequest {
                    endpoint,
                    params: parameters,
                    uri,
                    headers,
                    body,
                },
            )
            .await;
        }
        BackendEndpoint::ThreadUsage | BackendEndpoint::TurnEstimates => {
            return Err(crate::ApiError::openai(
                axum::http::StatusCode::CONFLICT,
                "api_error",
                "Monetary thread estimates are unavailable for virtual Token quotas.",
                Some("usage_estimate_unavailable"),
            ));
        }
    };
    let value =
        local.ok_or_else(|| crate::ApiError::internal("Missing virtual-account response"))?;
    Ok(crate::providers::chatgpt::identity::json_response(value))
}

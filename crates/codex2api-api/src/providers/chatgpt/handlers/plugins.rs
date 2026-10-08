//! Catalog reads use only the authenticated virtual account's plugin state.
use crate::{ApiError, ApiState, Result};
use axum::response::Response;
use codex2api_storage::VirtualAccess;
use serde_json::{Value, json};
use std::borrow::Cow;

pub(super) async fn list(
    state: &ApiState,
    access: &VirtualAccess,
    query: Option<&str>,
    installed: bool,
) -> Result<Response> {
    let pairs: Vec<_> = url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()).collect();
    let scope = parameter(&pairs, "scope")?.or(if installed { None } else { Some("GLOBAL") });
    if scope.is_some_and(|s| !matches!(s, "GLOBAL" | "USER" | "WORKSPACE")) {
        return Err(ApiError::bad_request("Invalid plugin scope."));
    }
    if let Some(collection) = parameter(&pairs, "collection")?
        && (installed || collection != "vertical" || scope != Some("GLOBAL"))
    {
        return Err(ApiError::bad_request("Invalid plugin collection."));
    }
    if parameter(&pairs, "includeDownloadUrls")?.is_some_and(|v| !matches!(v, "true" | "false")) {
        return Err(ApiError::bad_request("Invalid plugin download option."));
    }
    let limit = number(&pairs, "limit", 200)?;
    if !(1..=200).contains(&limit) {
        return Err(ApiError::bad_request(
            "Plugin limit must be between 1 and 200.",
        ));
    }
    let offset = number(&pairs, "pageToken", 0)?;
    let snapshot = state
        .storage
        .virtual_config(&access.virtual_account_id, "installed_plugins")
        .await?;
    let records = snapshot.value["plugins"]
        .as_array()
        .ok_or_else(|| ApiError::internal("Invalid virtual plugin records."))?;
    // The curated view is the same account-owned GLOBAL directory. It must not
    // fetch a supplier's curated catalog or invent a separate installation state.
    let matching: Vec<_> = records
        .iter()
        .filter(|p| scope.is_none_or(|scope| p["scope"] == scope))
        .filter(|p| installed || p["enabled"] == true)
        .collect();
    let next = offset.saturating_add(limit);
    let plugins: Vec<Value> = matching
        .iter()
        .skip(offset)
        .take(limit)
        .map(|p| {
            let mut value = (**p).clone();
            if !installed && let Some(object) = value.as_object_mut() {
                for key in ["enabled", "installed_at", "disabled_skill_names"] {
                    object.remove(key);
                }
            }
            value
        })
        .collect();
    Ok(super::super::identity::json_response(json!({
        "plugins": plugins,
        "pagination": {
            "limit": limit,
            "next_page_token": (next < matching.len()).then(|| next.to_string())
        }
    })))
}

fn parameter<'a>(pairs: &'a [(Cow<'_, str>, Cow<'_, str>)], name: &str) -> Result<Option<&'a str>> {
    let mut values = pairs.iter().filter(|(key, _)| key == name);
    let value = values.next().map(|(_, value)| value.as_ref());
    if values.next().is_some() {
        return Err(ApiError::bad_request("Duplicate plugin query parameter."));
    }
    Ok(value)
}

fn number(pairs: &[(Cow<'_, str>, Cow<'_, str>)], name: &str, default: usize) -> Result<usize> {
    parameter(pairs, name)?
        .map(|value| {
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ApiError::bad_request(
                    "Invalid plugin pagination parameter.",
                ));
            }
            value
                .parse()
                .map_err(|_| ApiError::bad_request("Invalid plugin pagination parameter."))
        })
        .unwrap_or(Ok(default))
}

use axum::{
    Json,
    extract::{Extension, OriginalUri, Path, Query, State},
    http::StatusCode,
    response::Response,
};
use chrono::Datelike;
use codex2api_storage::VirtualAccess;
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(serde::Deserialize)]
pub(crate) struct ReferralTrackingQuery {
    program_id: String,
    period: Option<String>,
    cursor: Option<usize>,
    limit: Option<usize>,
    account_id: Option<String>,
}

pub(crate) async fn referral_tracking(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Query(query): Query<ReferralTrackingQuery>,
) -> crate::Result<Response> {
    if let Some(id) = &query.account_id {
        account_match(&access, id)?;
    }
    if query.program_id.is_empty() || query.program_id.len() > 128 {
        return Err(crate::ApiError::bad_request("Invalid referral program."));
    }
    let limit = query.limit.unwrap_or(100);
    if !(1..=100).contains(&limit) {
        return Err(crate::ApiError::bad_request(
            "Referral page limit must be between 1 and 100.",
        ));
    }
    let now = chrono::Utc::now();
    let from = match query.period.as_deref().unwrap_or("past_90_days") {
        "past_90_days" => now - chrono::Duration::days(90),
        "this_month" => chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc(),
        _ => {
            return Err(crate::ApiError::bad_request(
                "Unsupported referral tracking period.",
            ));
        }
    };
    let saved = state
        .storage
        .virtual_config(&access.virtual_account_id, "referral_tracking")
        .await?;
    let mut rows: Vec<_> = saved.value["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let at = chrono::DateTime::parse_from_rfc3339(item["created_at"].as_str()?)
                .ok()?
                .with_timezone(&chrono::Utc);
            (item["program_id"] == query.program_id && at >= from && at <= now)
                .then(|| (at, item.clone()))
        })
        .collect();
    rows.sort_by(|a, b| {
        b.0.cmp(&a.0).then_with(|| {
            a.1["referral_id"]
                .as_str()
                .cmp(&b.1["referral_id"].as_str())
        })
    });
    let offset = query.cursor.unwrap_or(0);
    let total = rows.len();
    let items: Vec<_> = rows
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(_, mut item)| {
            // Tracking/importing an invite does not provide an email-sending service.
            item["can_resend"] = false.into();
            item
        })
        .collect();
    let cursor = if offset.saturating_add(limit) < total {
        Some((offset + limit).to_string())
    } else {
        None
    };
    Ok(crate::providers::chatgpt::identity::json_response(
        json!({"items":items,"cursor":cursor}),
    ))
}

pub(crate) async fn heartbeat(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> crate::Result<StatusCode> {
    // The renderer sends an empty POST and ignores the response body. This is
    // device liveness, not an upstream Sentinel attestation or authorization grant.
    state
        .storage
        .touch_virtual_access(&access.token_hash)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) fn account_match(access: &VirtualAccess, id: &str) -> crate::Result<()> {
    if id != access.virtual_account_id {
        return Err(crate::ApiError::openai(
            StatusCode::FORBIDDEN,
            "permission_error",
            "The account does not match this OAuth credential.",
            Some("account_mismatch"),
        ));
    }
    Ok(())
}

pub(crate) async fn read(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Extension(key): Extension<&'static str>,
    Path(params): Path<HashMap<String, String>>,
    OriginalUri(uri): OriginalUri,
) -> crate::Result<Response> {
    if let Some(id) = params.get("account_id") {
        account_match(&access, id)?;
    }
    let query: HashMap<_, _> = url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes())
        .into_owned()
        .collect();
    // Check every occurrence, including duplicate parameters.
    for (k, v) in url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()) {
        if k == "account_id" {
            account_match(&access, &v)?;
        }
    }
    let mut value = state
        .storage
        .virtual_config(&access.virtual_account_id, key)
        .await?
        .value;
    if matches!(key, "models" | "system_hints") {
        codex2api_storage::validate_virtual_config(key,&value).map_err(|_|crate::ApiError::openai(StatusCode::CONFLICT,"configuration_error","This account has an invalid legacy configuration. Review and save it in account administration.",Some("invalid_virtual_configuration")))?;
    }
    if key == "system_hints" {
        let mode = query.get("mode").map(String::as_str).unwrap_or("basic");
        if let Some(items) = value["system_hints"].as_array_mut() {
            items.retain(|h| match mode {
                "plugins" => h["hint_kind"] == "plugin",
                "connectors" => h["hint_kind"] == "connector",
                "basic" => h["hint_kind"].as_str().is_none_or(|k| k == "basic"),
                _ => false,
            });
        }
    }
    if key == "models" {
        crate::providers::chatgpt::identity::model_catalog(
            &state.storage,
            &access.virtual_account_id,
            &mut value,
        )
        .await?;
    }
    if key == "pricing" {
        let country = params
            .get("country_code")
            .ok_or_else(|| crate::ApiError::bad_request("Missing country code."))?;
        value = value.get(country).cloned().ok_or_else(|| {
            crate::ApiError::openai(
                StatusCode::CONFLICT,
                "configuration_error",
                "Configure this country's pricing in virtual-account administration.",
                Some("configuration_required"),
            )
        })?;
    }
    if key == "family" && value.is_null() {
        // The current-family reader expects an object with nullable membership.
        // A 404 clears its cache and enters the parental-controls error branch.
        value = json!({"id":null,"role":null});
    }
    if key == "account_settings" {
        let controls = state
            .storage
            .virtual_config(&access.virtual_account_id, "computer_use_policy")
            .await?
            .value;
        // Retain an explicit workspace denial as well as the service policy.
        value["beta_settings"]["windows_computer_use"] = (controls["computer_enabled"] == true
            && value["beta_settings"]["windows_computer_use"] != false)
            .into();
    }
    if key == "pins"
        && let Some(kind) = query.get("item_type")
    {
        value
            .as_array_mut()
            .unwrap()
            .retain(|p| p["item_type"].as_str() == Some(kind));
    }
    if matches!(key, "projects" | "notifications" | "automations") {
        let offset = query
            .get("cursor")
            .map(|s| s.parse::<usize>())
            .transpose()
            .map_err(|_| crate::ApiError::bad_request("Invalid cursor."))?
            .unwrap_or(0);
        let limit = query
            .get("limit")
            .map(|s| s.parse::<usize>())
            .transpose()
            .map_err(|_| crate::ApiError::bad_request("Invalid limit."))?
            .unwrap_or(20)
            .clamp(1, 100);
        if let Some(items) = value["items"].as_array_mut() {
            let total = items.len();
            *items = items.iter().skip(offset).take(limit).cloned().collect();
            value["cursor"] = if offset.saturating_add(limit) < total {
                json!((offset + limit).to_string())
            } else {
                Value::Null
            };
        }
    }
    Ok(crate::providers::chatgpt::identity::json_response(value))
}

pub(crate) async fn family_members(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> crate::Result<Response> {
    let family = state
        .storage
        .virtual_config(&access.virtual_account_id, "family")
        .await?
        .value;
    if family["id"].as_str() != Some(id.as_str()) {
        return Err(not_found());
    }
    let rows = family["members"].as_array().ok_or_else(|| {
        crate::ApiError::openai(
            StatusCode::CONFLICT,
            "api_error",
            "No member records have been synchronized for this family.",
            Some("family_records_unavailable"),
        )
    })?;
    let offset = query
        .get("offset")
        .map(|v| v.parse::<usize>())
        .transpose()
        .map_err(|_| crate::ApiError::bad_request("Invalid family offset."))?
        .unwrap_or(0);
    let limit = query
        .get("limit")
        .map(|v| v.parse::<usize>())
        .transpose()
        .map_err(|_| crate::ApiError::bad_request("Invalid family limit."))?
        .unwrap_or(rows.len().max(1))
        .clamp(1, 1000);
    Ok(crate::providers::chatgpt::identity::json_response(
        json!({"items":rows.iter().skip(offset).take(limit).collect::<Vec<_>>(),"total":rows.len(),"offset":offset,"limit":limit}),
    ))
}

pub(crate) async fn notifications_patch(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Json(body): Json<Value>,
) -> crate::Result<Response> {
    let saved = state
        .storage
        .virtual_config(&access.virtual_account_id, "notification_settings")
        .await?;
    let mut value = saved.value;
    let updates = body["updates"]
        .as_object()
        .ok_or_else(|| crate::ApiError::bad_request("Missing notification updates."))?;
    for (category, channels) in updates {
        let row = value["settings"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|s| s["category"] == *category)
            .ok_or_else(|| crate::ApiError::bad_request("Unknown notification category."))?;
        for (channel, enabled) in channels
            .as_object()
            .ok_or_else(|| crate::ApiError::bad_request("Invalid channels."))?
        {
            if !enabled.is_boolean() {
                return Err(crate::ApiError::bad_request(
                    "Invalid notification preference.",
                ));
            }
            let option = row["options"]
                .as_array_mut()
                .ok_or_else(|| crate::ApiError::bad_request("Invalid notification options."))?
                .iter_mut()
                .find(|o| o["channel"] == *channel)
                .ok_or_else(|| crate::ApiError::bad_request("Unknown notification channel."))?;
            option["enabled"] = enabled.clone();
        }
    }
    save_config(
        &state,
        &access,
        "notification_settings",
        &value,
        saved.revision,
    )
    .await?;
    Ok(crate::providers::chatgpt::identity::json_response(value))
}
pub(crate) async fn notification_reacted(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Path(id): Path<String>,
) -> crate::Result<Response> {
    let saved = state
        .storage
        .virtual_config(&access.virtual_account_id, "notifications")
        .await?;
    let mut value = saved.value;
    let item = value["items"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["id"] == id)
        .ok_or_else(not_found)?;
    item["reacted_to_at"] = chrono::Utc::now().to_rfc3339().into();
    save_config(&state, &access, "notifications", &value, saved.revision).await?;
    Ok(crate::providers::chatgpt::identity::json_response(
        json!({"success":true}),
    ))
}
pub(crate) async fn save_config(
    state: &crate::ApiState,
    access: &VirtualAccess,
    key: &str,
    value: &Value,
    revision: i64,
) -> crate::Result<()> {
    if state
        .storage
        .update_virtual_client_config(&access.virtual_account_id, key, value, revision)
        .await?
        .is_none()
    {
        return Err(crate::ApiError::openai(
            StatusCode::CONFLICT,
            "conflict_error",
            "Configuration changed; reload before saving.",
            Some("revision_conflict"),
        ));
    }
    Ok(())
}
pub(crate) async fn monthly_usage(
    Extension(access): Extension<VirtualAccess>,
    Path(id): Path<String>,
) -> crate::Result<Response> {
    account_match(&access, &id)?;
    // The client explicitly supports unavailable monetary caps. Local quotas are Token-based.
    Ok(detail(
        StatusCode::NOT_FOUND,
        "Current user monthly cap is not available.",
    ))
}
pub(crate) async fn conversation_init(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Json(body): Json<Value>,
) -> crate::Result<Response> {
    if !body.is_object() {
        return Err(crate::ApiError::bad_request(
            "Expected conversation object.",
        ));
    }
    for key in ["account_id", "conversation_owner_id"] {
        if let Some(v) = body.get(key).filter(|v| !v.is_null()) {
            account_match(
                &access,
                v.as_str()
                    .ok_or_else(|| crate::ApiError::bad_request("Invalid owner."))?,
            )?;
        }
    }
    let mut value = state
        .storage
        .virtual_config(&access.virtual_account_id, "conversation_metadata")
        .await?
        .value;
    let mut models = state
        .storage
        .virtual_config(&access.virtual_account_id, "models")
        .await?
        .value;
    crate::providers::chatgpt::identity::model_catalog(
        &state.storage,
        &access.virtual_account_id,
        &mut models,
    )
    .await?;
    let model = body["requested_default_model"]
        .as_str()
        .or(value["default_model_slug"].as_str())
        .or(models["default_model_slug"].as_str());
    let model = model
        .filter(|m| {
            models["models"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v["slug"].as_str() == Some(m)))
        })
        .ok_or_else(|| {
            crate::ApiError::openai(
                StatusCode::CONFLICT,
                "configuration_error",
                "Configure an available ChatGPT model for this virtual account.",
                Some("configuration_required"),
            )
        })?
        .to_owned();
    if let Some(id) = body["conversation_id"].as_str() {
        if state
            .storage
            .virtual_resource(&access.virtual_account_id, "conversation", id)
            .await?
            .is_none()
        {
            return Err(not_found());
        }
        value["conversation_id"] = id.into();
    }
    value["default_model_slug"] = model.into();
    // Initialization reads configured metadata. A conversation only exists after
    // upstream execution actually returns its ID; never fabricate one here.
    Ok(crate::providers::chatgpt::identity::json_response(value))
}
pub(crate) fn not_found() -> crate::ApiError {
    crate::ApiError::openai(
        StatusCode::NOT_FOUND,
        "not_found_error",
        "Resource not found in this virtual account.",
        Some("resource_not_found"),
    )
}
fn detail(status: StatusCode, message: &str) -> Response {
    let mut r = crate::providers::chatgpt::identity::json_response(json!({"detail":message}));
    *r.status_mut() = status;
    r
}

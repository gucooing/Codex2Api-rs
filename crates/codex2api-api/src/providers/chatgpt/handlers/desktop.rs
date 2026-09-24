//! Desktop reads for virtual-account state that must not inherit a supplier's private data.
use axum::{
    Json,
    extract::{Extension, Query, State},
    http::StatusCode,
    response::Response,
};
use codex2api_storage::VirtualAccess;
use serde::Deserialize;
use serde_json::json;

#[derive(Clone, Copy)]
pub(crate) enum DesktopEndpoint {
    Sites,
    Onboarding,
    Automations,
    Conversations,
    Beacons,
    VerifiedAccess,
    BrowserSettings,
    CodeReviewMetrics,
}

#[derive(Default, Deserialize)]
pub(crate) struct PageQuery {
    #[serde(default)]
    offset: u32,
    #[serde(default = "default_limit")]
    limit: u32,
    cursor: Option<usize>,
    filter: Option<String>,
    is_archived: Option<bool>,
    is_starred: Option<bool>,
    hide_snorlax: Option<bool>,
    conversation_origin: Option<String>,
    exclude_conversation_origin: Option<String>,
    order: Option<String>,
    expand: Option<bool>,
}
fn default_limit() -> u32 {
    20
}

pub(crate) async fn read(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Extension(endpoint): Extension<DesktopEndpoint>,
    Query(query): Query<PageQuery>,
) -> crate::Result<Response> {
    let key = match endpoint {
        DesktopEndpoint::Sites => "sites",
        DesktopEndpoint::VerifiedAccess => "verified_access",
        DesktopEndpoint::Onboarding => "onboarding",
        DesktopEndpoint::Automations => "automations",
        DesktopEndpoint::Beacons => "beacons",
        DesktopEndpoint::BrowserSettings => "browser_settings",
        DesktopEndpoint::Conversations | DesktopEndpoint::CodeReviewMetrics => "",
    };
    let value = if matches!(endpoint, DesktopEndpoint::Conversations) {
        let mut rows = state
            .storage
            .virtual_resources(&access.virtual_account_id, "conversation")
            .await?;
        rows.retain(|v| {
            v["is_visible"] != false
                && query
                    .is_archived
                    .is_none_or(|x| v["is_archived"].as_bool().unwrap_or(false) == x)
                && query
                    .is_starred
                    .is_none_or(|x| v["is_starred"].as_bool().unwrap_or(false) == x)
                && (query.hide_snorlax != Some(true) || v["gizmo_id"].is_null())
                && query
                    .conversation_origin
                    .as_ref()
                    .is_none_or(|x| v["conversation_origin"].as_str() == Some(x))
                && query
                    .exclude_conversation_origin
                    .as_ref()
                    .is_none_or(|x| v["conversation_origin"].as_str() != Some(x))
        });
        let order = match query.order.as_deref().unwrap_or("updated") {
            "updated" => "update_time",
            "created" => "create_time",
            _ => return Err(crate::ApiError::bad_request("Invalid conversation order.")),
        };
        rows.sort_by(|a, b| {
            b[order]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&a[order].as_f64().unwrap_or(0.0))
                .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        if query.expand == Some(true) {
            for row in &mut rows {
                if let Some(id) = row["id"].as_str()
                    && let Some((_, detail)) = state
                        .storage
                        .virtual_resource(&access.virtual_account_id, "conversation_detail", id)
                        .await?
                {
                    *row = detail;
                }
            }
        }

        let total = rows.len();
        let items: Vec<_> = rows
            .into_iter()
            .skip(query.offset as usize)
            .take(query.limit.clamp(1, 100) as usize)
            .collect();
        json!({"items":items,"total":total,"offset":query.offset,"limit":query.limit.clamp(1,100),"has_missing_conversations":false})
    } else if matches!(endpoint, DesktopEndpoint::Automations) {
        let saved = state
            .storage
            .virtual_config(&access.virtual_account_id, "automations")
            .await?;
        let filter = query.filter.as_deref().unwrap_or("all");
        if !matches!(
            filter,
            "all" | "scheduled" | "paused" | "active-and-paused" | "completed"
        ) {
            return Err(crate::ApiError::bad_request("Invalid automation filter."));
        }
        let mut items: Vec<_> = saved.value["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| {
                let enabled = a["is_enabled"]
                    .as_bool()
                    .or(a["enabled"].as_bool())
                    .unwrap_or(false);
                let completed = a["is_completed"] == true
                    || a["timing_mode"] != "condition_watch"
                        && a["next_run_times"].as_array().is_some_and(|t| t.is_empty());
                match filter {
                    "scheduled" => enabled && !completed,
                    "paused" => !enabled && !completed,
                    "active-and-paused" => !completed,
                    "completed" => completed,
                    _ => true,
                }
            })
            .cloned()
            .collect();
        items.sort_by(|a, b| {
            b["updated_at"]
                .as_str()
                .cmp(&a["updated_at"].as_str())
                .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        let total = items.len();
        let offset = query.cursor.unwrap_or(0);
        let limit = query.limit.clamp(1, 100) as usize;
        json!({"items":items.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"cursor":(offset.saturating_add(limit)<total).then(||(offset+limit).to_string())})
    } else if matches!(endpoint, DesktopEndpoint::CodeReviewMetrics) {
        json!({"data":state.storage.virtual_resources(&access.virtual_account_id,"code_review_metric").await?})
    } else {
        let saved = state
            .storage
            .virtual_config(&access.virtual_account_id, key)
            .await?;
        let mut value = saved.value;
        if matches!(endpoint, DesktopEndpoint::Beacons) {
            codex2api_storage::validate_virtual_config("beacons", &value).map_err(|_| {
                crate::ApiError::openai(
                    StatusCode::CONFLICT,
                    "configuration_error",
                    "Review and save the legacy announcement in account administration.",
                    Some("invalid_virtual_configuration"),
                )
            })?;
        }
        if matches!(endpoint, DesktopEndpoint::BrowserSettings) {
            value["revision"] = saved.revision.into();
        }
        value
    };
    Ok(crate::providers::chatgpt::identity::json_response(value))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrowserPatch {
    expected_revision: i64,
    #[serde(default)]
    preferences: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    rule_updates: Vec<RuleUpdate>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleUpdate {
    resource: String,
    pattern: String,
    decision: Option<String>,
}

fn conflict() -> crate::ApiError {
    crate::ApiError::openai(
        StatusCode::CONFLICT,
        "conflict_error",
        "Browser settings changed; reload before saving.",
        Some("revision_conflict"),
    )
}

pub(crate) async fn update_browser(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Json(patch): Json<BrowserPatch>,
) -> crate::Result<Response> {
    if patch.expected_revision < 0 || patch.rule_updates.len() > 1000 {
        return Err(crate::ApiError::bad_request(
            "Invalid browser settings update.",
        ));
    }
    let saved = state
        .storage
        .virtual_client_state(&access.virtual_account_id, "browser_settings")
        .await?;
    let revision = saved.as_ref().map_or(0, |s| s.revision);
    if revision != patch.expected_revision {
        return Err(conflict());
    }
    let mut value = saved.map(|s| s.value).unwrap_or_else(browser_settings);
    for (key, setting) in patch.preferences {
        let valid = match key.as_str() {
            "approval_mode" | "download_approval_mode" | "upload_approval_mode" => {
                matches!(setting.as_str(), Some("always_ask" | "never_ask"))
            }
            "history_approval_mode" | "iab_history_approval_mode" => matches!(
                setting.as_str(),
                Some("always_ask" | "never_ask" | "disabled")
            ),
            "disable_auto_review" | "full_cdp_access_enabled" | "webmcp_enabled" => {
                setting.is_boolean()
            }
            _ => false,
        };
        if !valid {
            return Err(crate::ApiError::bad_request("Invalid browser preference."));
        }
        value["preferences"][key] = setting;
    }
    for rule in patch.rule_updates {
        if !["origin", "download", "upload", "full_cdp"].contains(&rule.resource.as_str())
            || rule.pattern.is_empty()
            || rule.pattern.len() > 2048
            || rule.pattern.chars().any(char::is_control)
            || rule
                .decision
                .as_deref()
                .is_some_and(|v| v != "allow" && v != "deny")
        {
            return Err(crate::ApiError::bad_request("Invalid browser rule."));
        }
        let rules = value["rules"][&rule.resource]
            .as_object_mut()
            .ok_or_else(|| crate::ApiError::internal("Invalid stored browser rules."))?;
        if let Some(decision) = rule.decision {
            rules.insert(rule.pattern, decision.into());
        } else {
            rules.remove(&rule.pattern);
        }
    }
    if value.to_string().len() > 256 * 1024 {
        return Err(crate::ApiError::bad_request(
            "Browser settings are too large.",
        ));
    }
    let revision = state
        .storage
        .save_virtual_client_state(
            &access.virtual_account_id,
            "browser_settings",
            &value,
            Some(revision),
        )
        .await?
        .ok_or_else(conflict)?;
    value["revision"] = revision.into();
    Ok(crate::providers::chatgpt::identity::json_response(value))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OnboardingComplete {
    role: Option<String>,
    #[serde(default)]
    conversational_onboarding_skipped: bool,
    #[serde(default)]
    onboarding_credit_reward_warning_shown: bool,
}
pub(crate) async fn complete_onboarding(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Json(form): Json<OnboardingComplete>,
) -> crate::Result<Response> {
    if form
        .role
        .as_ref()
        .is_some_and(|role| role.len() > 128 || role.chars().any(char::is_control))
    {
        return Err(crate::ApiError::bad_request("Invalid onboarding role."));
    }
    let value = json!({"role":form.role,"desktop_onboarding_completed_at":chrono::Utc::now().to_rfc3339(),"conversational_onboarding_skipped":form.conversational_onboarding_skipped,"onboarding_credit_reward_warning_shown":form.onboarding_credit_reward_warning_shown});
    state
        .storage
        .save_virtual_client_state(&access.virtual_account_id, "onboarding", &value, None)
        .await?
        .ok_or_else(crate::ApiError::invalid_token)?;
    Ok(crate::providers::chatgpt::identity::json_response(
        json!({"success":true}),
    ))
}

fn browser_settings() -> serde_json::Value {
    // Defaults in this installed desktop's DD() browser-settings parser; no supplier rules.
    json!({"revision":0,"preferences":{
        "approval_mode":"always_ask","history_approval_mode":"always_ask","iab_history_approval_mode":"always_ask",
        "download_approval_mode":"always_ask","upload_approval_mode":"always_ask",
        "disable_auto_review":false,"full_cdp_access_enabled":false,"webmcp_enabled":true
    },"rules":{"origin":{},"download":{},"upload":{},"full_cdp":{}}})
}

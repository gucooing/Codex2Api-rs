//! The installed Desktop profile page uses /profiles/me/page, not the legacy WHAM DTO.
use axum::{
    Json,
    extract::{Extension, Path, State},
    response::Response,
};
use codex2api_storage::VirtualAccess;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

async fn page_value(state: &crate::ApiState, access: &VirtualAccess) -> crate::Result<Value> {
    let id = &access.virtual_account_id;
    let account = state
        .storage
        .virtual_account(id)
        .await?
        .ok_or_else(crate::ApiError::invalid_token)?;
    let private = state.storage.virtual_config(id, "profile").await?.value;
    let config = state.storage.virtual_config(id, "profile_page").await?;
    let summary = state.storage.virtual_usage_summary(id).await?;
    let events = state.storage.virtual_analytics(id, 0, i64::MAX).await?;
    let mut turns = BTreeSet::new();
    let mut threads = BTreeSet::new();
    let mut daily_turns = BTreeMap::<String, u64>::new();
    let mut plugins = BTreeMap::<String, (String, u64)>::new();
    let mut skills = BTreeSet::new();
    let mut skill_uses = 0_u64;
    for event in events {
        match event["event_type"].as_str() {
            Some("codex_turn_event") => {
                if let (Some(thread), Some(turn), Some(at)) = (
                    event["thread_id"].as_str(),
                    event["turn_id"].as_str(),
                    event["received_at_ms"]
                        .as_i64()
                        .and_then(chrono::DateTime::from_timestamp_millis),
                ) {
                    threads.insert(thread.to_owned());
                    if turns.insert((thread.to_owned(), turn.to_owned())) {
                        *daily_turns.entry(at.date_naive().to_string()).or_default() += 1;
                    }
                }
            }
            Some("codex_plugin_used") => {
                if let Some(plugin) = event["plugin_id"].as_str() {
                    let entry = plugins.entry(plugin.to_owned()).or_insert_with(|| {
                        (
                            event["plugin_name"].as_str().unwrap_or(plugin).to_owned(),
                            0,
                        )
                    });
                    entry.1 += 1;
                }
            }
            Some("skill_invocation") => {
                if let Some(skill) = event["skill_id"].as_str().or(event["skill_name"].as_str()) {
                    skills.insert(skill.to_owned());
                    skill_uses += 1;
                }
            }
            _ => {}
        }
    }
    let mut daily = BTreeMap::<String, Value>::new();
    for day in &summary.daily_usage_buckets {
        daily.insert(
            day.start_date.to_string(),
            json!({"start_date":day.start_date,"tokens":day.tokens,"chat_turns":0}),
        );
    }
    for (date, count) in daily_turns {
        daily
            .entry(date.clone())
            .or_insert_with(|| json!({"start_date":date,"tokens":0,"chat_turns":0}))["chat_turns"] =
            count.into();
    }
    let daily: Vec<_> = daily.into_values().collect();
    let mut cumulative = Vec::new();
    let (mut tokens, mut chats) = (0_i64, 0_u64);
    let mut weekly = BTreeMap::<String, (i64, u64)>::new();
    for day in &daily {
        tokens += day["tokens"].as_i64().unwrap_or(0);
        chats += day["chat_turns"].as_u64().unwrap_or(0);
        cumulative.push(json!({"start_date":day["start_date"],"tokens":tokens,"chat_turns":chats}));
        if let Some(date) = day["start_date"]
            .as_str()
            .and_then(|s| s.parse::<chrono::NaiveDate>().ok())
        {
            use chrono::Datelike;
            let start = date - chrono::Duration::days(date.weekday().num_days_from_sunday() as i64);
            let counts = weekly.entry(start.to_string()).or_default();
            counts.0 += day["tokens"].as_i64().unwrap_or(0);
            counts.1 += day["chat_turns"].as_u64().unwrap_or(0);
        }
    }
    let top_plugins: Vec<_> = plugins.into_iter().map(|(id,(name,count))| json!({"plugin_id":id,"plugin_name":name,"usage_count":count,"logo_url":null})).collect();
    Ok(json!({
        "is_self":true,"can_edit":true,
        "profile_details":{
            "id":account.id,"display_name":config.value["display_name"].as_str().filter(|s|!s.trim().is_empty()).unwrap_or(&account.name),
            "username":config.value["username"].as_str().filter(|s|!s.trim().is_empty()).unwrap_or(&account.username),
            "profile_picture_url":private["picture"].as_str().map(str::trim),"description":private["bio"],"photo_frame_style":config.value["photo_frame_style"]
        },
        "page":{
            "page_version":config.revision.to_string(),"visibility":{"value":"private"},"display_settings":config.value["display_settings"],
            "stats":{"current_streak_days":summary.current_streak_days,"longest_streak_days":summary.longest_streak_days,"agentic":{
                "lifetime_tokens":summary.lifetime_tokens,"peak_daily_tokens":summary.peak_daily_tokens,"longest_running_turn_sec":summary.longest_running_turn_sec
            }},
            "activity_graph":{"daily_usage_buckets":daily,"cumulative_daily_usage_buckets":cumulative,"weekly_usage_buckets":weekly.into_iter().map(|(date,(tokens,chats))|json!({"start_date":date,"tokens":tokens,"chat_turns":chats})).collect::<Vec<_>>()},
            "insights":{"agentic":{"total_threads":threads.len(),"fast_mode_usage_percentage":null,"most_used_reasoning_effort":null,"most_used_reasoning_effort_percentage":null},"total_skills_used":skill_uses,"unique_skills_used":skills.len()},
            "top_plugins":top_plugins
        }
    }))
}

pub(crate) async fn read(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> crate::Result<Response> {
    Ok(crate::providers::chatgpt::identity::json_response(
        page_value(&state, &access).await?,
    ))
}

pub(crate) async fn by_username(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Path(username): Path<String>,
) -> crate::Result<Response> {
    let value = page_value(&state, &access).await?;
    // These profiles are private to their virtual account, including username routes.
    if value["profile_details"]["username"] != username {
        return Err(super::virtual_data::not_found());
    }
    Ok(crate::providers::chatgpt::identity::json_response(value))
}

pub(crate) async fn update_page(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Json(patch): Json<Value>,
) -> crate::Result<Response> {
    let mut saved = state
        .storage
        .virtual_config(&access.virtual_account_id, "profile_page")
        .await?;
    let fields = patch
        .as_object()
        .filter(|p| p.len() == 1)
        .and_then(|p| p.get("display_settings"))
        .and_then(Value::as_object)
        .ok_or_else(|| crate::ApiError::bad_request("Expected profile display settings."))?;
    for (key, value) in fields {
        if saved.value["display_settings"].get(key).is_none() || !value.is_boolean() {
            return Err(crate::ApiError::bad_request(
                "Invalid profile display setting.",
            ));
        }
        saved.value["display_settings"][key] = value.clone();
    }
    super::virtual_data::save_config(
        &state,
        &access,
        "profile_page",
        &saved.value,
        saved.revision,
    )
    .await?;
    Ok(crate::providers::chatgpt::identity::json_response(
        page_value(&state, &access).await?["page"].clone(),
    ))
}

pub(crate) async fn update_profile(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Json(patch): Json<Value>,
) -> crate::Result<Response> {
    let fields = patch
        .as_object()
        .ok_or_else(|| crate::ApiError::bad_request("Expected profile fields."))?;
    let mut page = state
        .storage
        .virtual_config(&access.virtual_account_id, "profile_page")
        .await?;
    let mut private = state
        .storage
        .virtual_config(&access.virtual_account_id, "profile")
        .await?;
    for (key, value) in fields {
        let text = value
            .as_str()
            .ok_or_else(|| crate::ApiError::bad_request("Invalid profile text."))?;
        if text.len() > 2000 || text.chars().any(char::is_control) {
            return Err(crate::ApiError::bad_request("Invalid profile text."));
        }
        match key.as_str() {
            "display_name" | "username" | "photo_frame_style" => {
                page.value[key] = text.trim().into()
            }
            "description" => private.value["bio"] = text.trim().into(),
            _ => return Err(crate::ApiError::bad_request("Unsupported profile field.")),
        }
    }
    codex2api_storage::validate_virtual_config("profile_page", &page.value)
        .map_err(crate::ApiError::bad_request)?;
    if !state
        .storage
        .update_virtual_profile(&access.virtual_account_id, &page, &private)
        .await?
    {
        return Err(crate::ApiError::openai(
            axum::http::StatusCode::CONFLICT,
            "conflict_error",
            "Profile changed; reload and retry.",
            Some("revision_conflict"),
        ));
    }
    Ok(crate::providers::chatgpt::identity::json_response(
        page_value(&state, &access).await?["profile_details"].clone(),
    ))
}

pub(crate) async fn update_legacy(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Json(patch): Json<Value>,
) -> crate::Result<Response> {
    update_profile(State(state.clone()), Extension(access.clone()), Json(patch)).await?;
    let page = page_value(&state, &access).await?;
    let stats = state
        .storage
        .virtual_usage_summary(&access.virtual_account_id)
        .await?;
    Ok(crate::providers::chatgpt::identity::json_response(
        json!({"profile":page["profile_details"],"stats":stats,"metadata":{"stats_error":null}}),
    ))
}

//! Usage settings contracts consumed by the installed desktop client.
//! Supplier quota synchronization does not expose the supplier's historical activity.
use axum::{
    extract::{Extension, Query, State},
    response::Response,
};
use chrono::{Days, NaiveDate, Utc};
use codex2api_storage::VirtualAccess;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(crate) enum UsageEndpoint {
    WorkspaceCounts,
    Tokens,
    Credits,
    Plugins,
    PlanHistory,
    Skills,
}

#[derive(Deserialize)]
pub(crate) struct UsageQuery {
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    group_by: Option<String>,
}

pub(crate) async fn read(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Extension(endpoint): Extension<UsageEndpoint>,
    Query(query): Query<UsageQuery>,
) -> crate::Result<Response> {
    let end = query.end_date.unwrap_or_else(|| Utc::now().date_naive());
    let start = query
        .start_date
        .or_else(|| end.checked_sub_days(Days::new(6)))
        .ok_or_else(|| crate::ApiError::bad_request("Invalid start date."))?;
    if start > end
        || query
            .group_by
            .as_deref()
            .is_some_and(|group| group != "day")
    {
        return Err(crate::ApiError::bad_request(
            "Invalid daily usage range or grouping.",
        ));
    }
    let value = match endpoint {
        UsageEndpoint::Tokens => {
            let until = end
                .succ_opt()
                .ok_or_else(|| crate::ApiError::bad_request("Invalid end date."))?;
            let rows = state
                .storage
                .virtual_daily_model_tokens(
                    &access.virtual_account_id,
                    start
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp_millis(),
                    until
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp_millis(),
                )
                .await?;
            let mut days: BTreeMap<NaiveDate, (i64, Vec<Value>)> = BTreeMap::new();
            for row in rows {
                let (total, models) = days.entry(row.date).or_default();
                *total += row.tokens;
                // The personal-history client uses `credits` for the amount in `units`,
                // including when units is tokens. Cached input is already part of input.
                models.push(json!({"model":row.model,"credits":row.tokens}));
            }
            let data: Vec<_> = days
                .into_iter()
                .map(|(date, (total, models))| {
                    json!({
                        "date":date,"models":models,"product_surface_usage_values":{"codex":total}
                    })
                })
                .collect();
            json!({"units":"tokens","data":data,"data_freshness_ts":null})
        }
        UsageEndpoint::WorkspaceCounts | UsageEndpoint::Plugins | UsageEndpoint::Skills => {
            let until = end
                .succ_opt()
                .ok_or_else(|| crate::ApiError::bad_request("Invalid end date."))?;
            let rows = state
                .storage
                .virtual_analytics(
                    &access.virtual_account_id,
                    start
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp_millis(),
                    until
                        .and_hms_opt(0, 0, 0)
                        .unwrap()
                        .and_utc()
                        .timestamp_millis(),
                )
                .await?;
            aggregate_events(endpoint, rows)
        }
        UsageEndpoint::Credits => {
            json!({"data":state.storage.virtual_resources(&access.virtual_account_id,"credit_event").await?.into_iter().filter(|v|v["date"].as_str().is_some_and(|d|d>=start.to_string().as_str()&&d<=end.to_string().as_str())).collect::<Vec<_>>(),"data_freshness_ts":null})
        }
        UsageEndpoint::PlanHistory => json!({
            "data_as_of":null,"coverage_start":null,"coverage_complete":false,
            "approximate":true,"boundary_tolerance_seconds":null,"periods":state.storage.virtual_resources(&access.virtual_account_id,"plan_period").await?
        }),
    };
    Ok(crate::providers::chatgpt::identity::json_response(value))
}

fn aggregate_events(endpoint: UsageEndpoint, rows: Vec<Value>) -> Value {
    use std::collections::BTreeSet;
    let mut days: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for row in rows {
        let kind = row["event_type"].as_str().unwrap_or("");
        let accept = match endpoint {
            UsageEndpoint::WorkspaceCounts => kind == "codex_turn_event",
            UsageEndpoint::Plugins => kind == "codex_plugin_used",
            UsageEndpoint::Skills => kind == "skill_invocation",
            _ => false,
        };
        if !accept {
            continue;
        }
        if matches!(endpoint, UsageEndpoint::WorkspaceCounts) {
            let (Some(thread), Some(turn)) = (row["thread_id"].as_str(), row["turn_id"].as_str())
            else {
                continue;
            };
            if !seen.insert((thread.to_owned(), turn.to_owned())) {
                continue;
            }
        }
        if let Some(at) = row["received_at_ms"]
            .as_i64()
            .and_then(chrono::DateTime::from_timestamp_millis)
        {
            days.entry(at.date_naive().to_string())
                .or_default()
                .push(row);
        }
    }
    let data:Vec<_>=days.into_iter().map(|(date,rows)|{
        let mut counts:BTreeMap<String,i64>=BTreeMap::new();
        let mut clients:BTreeMap<String,i64>=BTreeMap::new();
        for row in &rows {
            let name=match endpoint {UsageEndpoint::WorkspaceCounts=>row["model"].as_str().or(row["model_slug"].as_str()).unwrap_or("other"),UsageEndpoint::Plugins=>row["plugin_name"].as_str().or(row["plugin_id"].as_str()).unwrap_or("other"),_=>row["skill_name"].as_str().unwrap_or("other")};
            *counts.entry(name.into()).or_default()+=1;
            *clients.entry(row["product_client_id"].as_str().unwrap_or("other").into()).or_default()+=1;
        }
        if matches!(endpoint,UsageEndpoint::WorkspaceCounts){json!({"date":date,"totals":{"turns":rows.len()},"models":counts.into_iter().map(|(model,turns)|json!({"model":model,"turns":turns})).collect::<Vec<_>>(),"clients":clients.into_iter().map(|(client_id,turns)|json!({"client_id":client_id,"turns":turns})).collect::<Vec<_>>()})}
        else {let field=if matches!(endpoint,UsageEndpoint::Skills){"skill_usage_overviews"}else{"plugin_usage_overviews"};json!({"date":date,field:counts.into_iter().map(|(name,count)|json!({"display_name":name,"skill_name":name,"invocation_counts":count})).collect::<Vec<_>>()})}
    }).collect();
    json!({"data":data,"data_freshness_ts":null})
}

//! Official quota groups and windows, in the order returned to Antigravity Desktop.
use crate::{Result, UpstreamError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    #[serde(default)]
    groups: Vec<Group>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Group {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    buckets: Vec<Bucket>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bucket {
    #[serde(default)]
    bucket_id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    window: String,
    remaining_fraction: Option<f64>,
    reset_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    disabled: bool,
}

#[derive(Debug, Serialize)]
pub struct QuotaGroup {
    pub name: String,
    pub windows: Vec<QuotaWindow>,
}

#[derive(Debug, Serialize)]
pub struct QuotaWindow {
    pub id: String,
    pub name: String,
    pub window: String,
    pub remaining_fraction: Option<f64>,
    pub reset_at: Option<i64>,
    pub disabled: bool,
}

fn parse_summary(value: &Value) -> Result<Summary> {
    let summary: Summary = serde_json::from_value(value.clone()).map_err(|_| {
        UpstreamError::InvalidRequest("Antigravity returned an invalid quota summary".into())
    })?;
    if summary.groups.len() > 128
        || summary.groups.iter().any(|group| {
            group.buckets.len() > 128
                || group.buckets.iter().any(|bucket| {
                    bucket.remaining_fraction.is_some_and(|fraction| {
                        !fraction.is_finite() || !(0.0..=1.0).contains(&fraction)
                    })
                })
        })
    {
        return Err(UpstreamError::InvalidRequest(
            "Antigravity returned an invalid quota summary".into(),
        ));
    }
    Ok(summary)
}

pub(super) fn validate_summary(value: &Value) -> Result<()> {
    parse_summary(value).map(|_| ())
}

pub fn quota_groups(snapshot: &Value) -> Vec<QuotaGroup> {
    let Ok(summary) = parse_summary(&snapshot["quota_summary"]) else {
        return Vec::new();
    };
    summary
        .groups
        .into_iter()
        .filter(|group| !group.buckets.is_empty())
        .map(|group| QuotaGroup {
            name: group.display_name,
            windows: group
                .buckets
                .into_iter()
                .map(|bucket| QuotaWindow {
                    id: bucket.bucket_id,
                    name: bucket.display_name,
                    window: bucket.window,
                    // This field is a protobuf oneof: absent is unknown, not zero.
                    remaining_fraction: bucket.remaining_fraction,
                    reset_at: bucket.reset_time.map(|time| time.timestamp()),
                    disabled: bucket.disabled,
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_official_groups_windows_and_unknown_measurements() {
        let snapshot = json!({"quota_summary":{"groups":[
            {"displayName":"Gemini Models","buckets":[
                {"bucketId":"gemini-weekly","window":"weekly","remainingFraction":0.8},
                {"bucketId":"gemini-5h","window":"5h","remainingFraction":1}
            ]},
            {"displayName":"Claude and GPT models","buckets":[
                {"bucketId":"3p-weekly","window":"weekly","remainingFraction":0},
                {"bucketId":"3p-5h","window":"5h","remainingFraction":0.5}
            ]},
            {"displayName":"Gemini 4 Argon","buckets":[
                {"bucketId":"gemini-argon-wtu-weekly","window":"weekly","remainingFraction":0.93,
                 "resetTime":"2026-10-24T01:28:50Z"}
            ]},
            {"displayName":"Other","buckets":[
                {"remainingAmount":"20","resetTime":"2026-10-24T01:28:50Z","disabled":true}
            ]}
        ]}});
        let groups = quota_groups(&snapshot);
        assert_eq!(
            groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
            [
                "Gemini Models",
                "Claude and GPT models",
                "Gemini 4 Argon",
                "Other"
            ]
        );
        assert_eq!(groups[0].windows.len(), 2);
        assert_eq!(groups[1].windows[0].remaining_fraction, Some(0.0));
        assert_eq!(groups[2].windows.len(), 1);
        assert_eq!(groups[2].windows[0].remaining_fraction, Some(0.93));
        assert!(groups[2].windows[0].reset_at.is_some());
        assert_eq!(groups[3].windows[0].remaining_fraction, None);
        assert!(groups[3].windows[0].disabled);
        assert!(
            validate_summary(&json!({"groups":[{"buckets":[{"remainingFraction":-1}]}]})).is_err()
        );
    }
}

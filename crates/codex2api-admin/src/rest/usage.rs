use super::error::{ApiError, ApiResult};
use crate::AdminState;
use axum::{
    Json,
    extract::{Query, State},
};
use codex2api_storage::UsageFilter;
use serde::Deserialize;
#[derive(Default, Deserialize)]
pub(crate) struct Filters {
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub supplier_id: String,
    #[serde(default)]
    pub virtual_account: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub until: String,
    #[serde(default)]
    pub tz_offset: i32,
    #[serde(default)]
    pub page: u32,
    pub page_size: Option<u32>,
}

impl Filters {
    pub fn storage_filter(&self) -> Result<UsageFilter, String> {
        codex2api_storage::table_page_size(self.page_size).map_err(|e| e.to_string())?;
        let parse = |value: &str| -> Result<Option<i64>, String> {
            if value.is_empty() {
                return Ok(None);
            }
            let dt = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
                .map_err(|_| "时间格式无效".to_string())?;
            // Browser getTimezoneOffset is minutes west of UTC.
            if !(-840..=840).contains(&self.tz_offset) {
                return Err("时区无效".into());
            }
            let utc = dt
                .and_utc()
                .checked_add_signed(chrono::TimeDelta::minutes(i64::from(self.tz_offset)))
                .ok_or_else(|| "时间超出范围".to_string())?;
            Ok(Some(utc.timestamp_millis()))
        };
        let from_ms = parse(&self.from)?;
        let until_ms = parse(&self.until)?;
        if matches!((from_ms,until_ms),(Some(a),Some(b)) if a>=b) {
            return Err("结束时间必须晚于开始时间".into());
        }
        if !matches!(
            self.status.as_str(),
            "" | "in_progress" | "completed" | "failed" | "incomplete" | "interrupted"
        ) {
            return Err("状态无效".into());
        }
        Ok(UsageFilter {
            account: Some(self.account.trim().into()),
            account_id: Some(self.supplier_id.clone()),
            subject_id: Some(self.virtual_account.clone()),
            model: Some(self.model.trim().into()),
            status: Some(self.status.clone()),
            from_ms,
            until_ms,
            page: self.page,
            page_size: self.page_size,
        })
    }
    #[cfg(test)]
    pub fn query_for_page(&self, page: u32) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query
            .extend_pairs([
                ("account", self.account.as_str()),
                ("supplier_id", self.supplier_id.as_str()),
                ("virtual_account", &self.virtual_account),
                ("model", &self.model),
                ("status", &self.status),
                ("from", &self.from),
                ("until", &self.until),
            ])
            .append_pair("tz_offset", &self.tz_offset.to_string())
            .append_pair("page_size", &self.page_size.unwrap_or(20).to_string())
            .append_pair("page", &page.to_string());
        query.finish()
    }
}

pub async fn page(State(s): State<AdminState>, Query(f): Query<Filters>) -> ApiResult {
    let filter = f.storage_filter().map_err(ApiError::bad)?;
    let page = s.storage.query_usage(&filter).await?;
    Ok(Json(super::dto::value(super::dto::Usage {
        records: page.records,
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    })))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_times_map_to_utc_and_pagination_preserves_filters() {
        let f = Filters {
            from: "2026-09-16T08:00".into(),
            until: "2026-09-17T08:00".into(),
            tz_offset: -480,
            model: "gpt & test".into(),
            status: "failed".into(),
            ..Default::default()
        };
        let stored = f.storage_filter().unwrap();
        assert_eq!(stored.status.as_deref(), Some("failed"));
        assert_eq!(
            stored.from_ms,
            Some(
                chrono::DateTime::parse_from_rfc3339("2026-09-16T00:00:00Z")
                    .unwrap()
                    .timestamp_millis()
            )
        );
        assert!(f.query_for_page(2).contains("model=gpt+%26+test"));
        assert!(f.query_for_page(2).contains("status=failed"));
        assert!(f.query_for_page(2).contains("page=2"));
        let bad = Filters {
            from: f.until,
            until: f.from,
            ..Default::default()
        };
        assert!(bad.storage_filter().is_err());
    }
}

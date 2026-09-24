//! Virtual-account-owned graduation/unlink notice history and client dismissals.
//! Only a family business event may create a notice; no administrator form does.
use crate::{Result, Storage, StorageError, VirtualAccess};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const KIND: &str = "family_graduation_notice";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FamilyNoticeRecipient {
    Teen,
    Parent,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct FamilyGraduationNotice {
    pub id: String,
    pub source_event_id: String,
    pub recipient_type: FamilyNoticeRecipient,
    pub teen_display_name: String,
    pub learn_more_url: Option<String>,
    pub created_at_ms: i64,
    pub dismissed_at_ms: Option<i64>,
    pub dismissed_by_device_id: Option<String>,
}

impl FamilyGraduationNotice {
    pub fn client_value(&self) -> Value {
        json!({"id":self.id,"recipient_type":self.recipient_type,"teen_display_name":self.teen_display_name,"learn_more_url":self.learn_more_url})
    }
}

impl Storage {
    /// Record an actual local family event once. This does not perform or claim
    /// age verification, unlink accounts, or import a supplier's family data.
    pub async fn record_family_graduation_notice(
        &self,
        owner: &str,
        source_event_id: &str,
        recipient_type: FamilyNoticeRecipient,
        teen_display_name: &str,
        learn_more_url: Option<&str>,
    ) -> Result<String> {
        if source_event_id.trim().is_empty()
            || source_event_id.len() > 256
            || teen_display_name.trim().is_empty()
            || teen_display_name.len() > 256
            || learn_more_url.is_some_and(|s| {
                !url::Url::parse(s).is_ok_and(|u| {
                    matches!(u.scheme(), "http" | "https")
                        && u.username().is_empty()
                        && u.password().is_none()
                })
            })
        {
            return Err(StorageError::Constraint(
                "Invalid family event notice".into(),
            ));
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let recipient = match recipient_type {
            FamilyNoticeRecipient::Teen => "teen",
            FamilyNoticeRecipient::Parent => "parent",
        };
        let existing:Option<String> = sqlx::query_scalar("SELECT id FROM virtual_resources WHERE virtual_account_id=? AND kind=? AND json_extract(value_json,'$.source_event_id')=? AND json_extract(value_json,'$.recipient_type')=?")
            .bind(owner).bind(KIND).bind(source_event_id).bind(recipient).fetch_optional(&mut *tx).await?;
        if let Some(id) = existing {
            tx.commit().await?;
            return Ok(id);
        }
        let now = chrono::Utc::now().timestamp_millis();
        let notice = FamilyGraduationNotice {
            id: uuid::Uuid::new_v4().to_string(),
            source_event_id: source_event_id.into(),
            recipient_type,
            teen_display_name: teen_display_name.into(),
            learn_more_url: learn_more_url.map(str::to_owned),
            created_at_ms: now,
            dismissed_at_ms: None,
            dismissed_by_device_id: None,
        };
        sqlx::query("INSERT INTO virtual_resources(virtual_account_id,kind,id,value_json,created_at_ms,updated_at_ms) VALUES(?,?,?,?,?,?)")
            .bind(owner).bind(KIND).bind(&notice.id).bind(serde_json::to_string(&notice)?).bind(now).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(notice.id)
    }

    pub async fn family_graduation_notices(
        &self,
        owner: &str,
        include_dismissed: bool,
    ) -> Result<Vec<FamilyGraduationNotice>> {
        let rows:Vec<String> = sqlx::query_scalar("SELECT value_json FROM virtual_resources WHERE virtual_account_id=? AND kind=? AND (? OR json_extract(value_json,'$.dismissed_at_ms') IS NULL) ORDER BY created_at_ms,id")
            .bind(owner).bind(KIND).bind(include_dismissed).fetch_all(self.pool()).await?;
        rows.into_iter()
            .map(|s| serde_json::from_str(&s).map_err(StorageError::from))
            .collect()
    }

    /// Capture only the requested IDs. Newer notices and all other accounts are
    /// preserved; a mixed owned/unknown batch changes nothing. Repeats are safe.
    pub async fn dismiss_family_graduation_notices(
        &self,
        access: &VirtualAccess,
        ids: &[String],
    ) -> Result<bool> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let mut notices = Vec::new();
        let ids: std::collections::BTreeSet<_> = ids.iter().collect();
        for id in ids {
            let raw:Option<String> = sqlx::query_scalar("SELECT value_json FROM virtual_resources WHERE virtual_account_id=? AND kind=? AND id=?")
                .bind(&access.virtual_account_id).bind(KIND).bind(id).fetch_optional(&mut *tx).await?;
            let Some(raw) = raw else {
                tx.rollback().await?;
                return Ok(false);
            };
            notices.push(serde_json::from_str::<FamilyGraduationNotice>(&raw)?);
        }
        let now = chrono::Utc::now().timestamp_millis();
        for mut notice in notices {
            if notice.dismissed_at_ms.is_some() {
                continue;
            }
            notice.dismissed_at_ms = Some(now);
            notice.dismissed_by_device_id = Some(access.device_id.clone());
            sqlx::query("UPDATE virtual_resources SET value_json=?,updated_at_ms=? WHERE virtual_account_id=? AND kind=? AND id=?")
                .bind(serde_json::to_string(&notice)?).bind(now).bind(&access.virtual_account_id).bind(KIND).bind(&notice.id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(true)
    }
}

use crate::{Result, UpstreamError};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

/// Pinned Codex core/context/world_state/environment.rs sends these as a user
/// environment_context fragment, not HTTP headers or client_metadata fields.
pub fn apply_response_timezone(body: &mut Value, timezone: Option<&str>) -> Result<()> {
    apply_at(body, timezone, Utc::now())
}

fn apply_at(body: &mut Value, timezone: Option<&str>, now: DateTime<Utc>) -> Result<()> {
    let Some(timezone) = timezone.filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let zone = timezone
        .parse::<chrono_tz::Tz>()
        .map_err(|_| UpstreamError::InvalidRequest("Invalid account timezone".into()))?;
    let date = now.with_timezone(&zone).format("%Y-%m-%d").to_string();
    let object = body
        .as_object_mut()
        .ok_or_else(|| UpstreamError::InvalidRequest("Request body must be an object".into()))?;
    let input = object.entry("input").or_insert_with(|| json!([]));
    if let Some(text) = input.as_str() {
        *input = json!([{"role":"user", "content":text}]);
    }
    let Some(items) = input.as_array_mut() else {
        return Ok(());
    };
    // Only update the latest environment fragment. Earlier conversation history,
    // tool output and assistant messages retain their original contents.
    for item in items.iter_mut().rev() {
        if item.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(content) = item.get_mut("content") else {
            continue;
        };
        if replace_context(content, timezone, &date) {
            return Ok(());
        }
        if let Some(parts) = content.as_array_mut() {
            for part in parts.iter_mut().rev() {
                if part.get("type").and_then(Value::as_str) == Some("input_text")
                    && let Some(text) = part.get_mut("text")
                    && replace_context(text, timezone, &date)
                {
                    return Ok(());
                }
            }
        }
    }
    let fragment = format!(
        "<environment_context>\n  <current_date>{date}</current_date>\n  <timezone>{timezone}</timezone>\n</environment_context>"
    );
    let index = items
        .iter()
        .rposition(|item| item.get("role").and_then(Value::as_str) == Some("user"))
        .unwrap_or(items.len());
    items.insert(index, json!({"type":"message", "role":"user", "content":[{"type":"input_text", "text":fragment}]}));
    Ok(())
}

fn replace_context(value: &mut Value, timezone: &str, date: &str) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    let Some(start) = text.rfind("<environment_context>") else {
        return false;
    };
    let Some(end) = text[start..]
        .find("</environment_context>")
        .map(|end| start + end)
    else {
        return false;
    };
    let mut fragment = text[start..end].to_string();
    for (tag, replacement) in [("current_date", date), ("timezone", timezone)] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        if let Some(from) = fragment.find(&open).map(|from| from + open.len())
            && let Some(to) = fragment[from..].find(&close).map(|to| from + to)
        {
            fragment.replace_range(from..to, replacement);
        } else {
            if !fragment.ends_with('\n') {
                fragment.push('\n');
            }
            fragment.push_str(&format!("  {open}{replacement}{close}\n"));
        }
    }
    let mut updated = text.to_string();
    updated.replace_range(start..end, &fragment);
    *value = Value::String(updated);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_official_fragment_and_local_date_without_changing_other_content() {
        let now = DateTime::parse_from_rfc3339("2026-09-17T01:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let context = "<environment_context>\n  <cwd>/work</cwd>\n  <current_date>2026-09-17</current_date>\n  <timezone>Asia/Taipei</timezone>\n</environment_context>";
        let mut body = json!({"input":[
            {"role":"user", "content":context},
            {"type":"function_call_output", "output":context},
            {"role":"user", "content":[{"type":"input_text", "text":context}, {"type":"input_text", "text":"keep my question"}]}],
            "previous_response_id":"previous", "client_metadata":{"thread_id":"thread"}});
        let original = body.clone();
        apply_at(&mut body, None, now).unwrap();
        assert_eq!(body, original);
        apply_at(&mut body, Some("America/Los_Angeles"), now).unwrap();
        let text = body["input"][2]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("<current_date>2026-09-16</current_date>"));
        assert!(text.contains("<timezone>America/Los_Angeles</timezone>"));
        assert!(text.contains("<cwd>/work</cwd>"));
        assert_eq!(body["input"][0], original["input"][0]);
        assert_eq!(body["input"][1], original["input"][1]);
        assert_eq!(
            body["input"][2]["content"][1],
            original["input"][2]["content"][1]
        );
        assert_eq!(body["client_metadata"], original["client_metadata"]);
        assert_eq!(body["previous_response_id"], "previous");
        let once = body.clone();
        apply_at(&mut body, Some("America/Los_Angeles"), now).unwrap();
        assert_eq!(body, once);
    }

    #[test]
    fn adds_context_for_plain_input_and_incremental_requests() {
        for input in [json!("question"), json!([])] {
            let mut body = json!({"input":input, "previous_response_id":"previous"});
            apply_response_timezone(&mut body, Some("Asia/Taipei")).unwrap();
            assert!(
                body["input"][0]["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("<timezone>Asia/Taipei</timezone>")
            );
            if input.is_string() {
                assert_eq!(body["input"][1]["content"], "question");
            }
        }
        let mut body = json!({"previous_response_id":"previous"});
        apply_response_timezone(&mut body, Some("Asia/Taipei")).unwrap();
        assert_eq!(body["input"].as_array().unwrap().len(), 1);
    }
}

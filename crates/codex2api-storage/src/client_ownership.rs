//! Management may change service policy, not manufacture client activity or choices.
use serde_json::Value;

/// Only client-owned actions are writable with a consumer OAuth token.
pub fn client_writable(key: &str) -> bool {
    matches!(
        key,
        "user_settings"
            | "cloud_preferences"
            | "profile_page"
            | "pins"
            | "notification_settings"
            | "notifications"
            | "voice"
            | "browser_settings"
            | "onboarding"
    )
}

pub fn validate_client_change(
    key: &str,
    before: &Value,
    after: &Value,
) -> std::result::Result<(), String> {
    if !client_writable(key) {
        return Err("客户端无权修改服务配置".into());
    }
    if key == "user_settings" {
        let (mut old, mut new) = (before.clone(), after.clone());
        for value in [&mut old, &mut new] {
            if let Some(object) = value.as_object_mut() {
                object.remove("settings");
            }
        }
        if old != new {
            return Err("客户端只能修改自己的偏好，不能修改服务权限".into());
        }
    }
    if key == "notifications" {
        let (mut old, mut new) = (before.clone(), after.clone());
        for value in [&mut old, &mut new] {
            for item in value["items"].as_array_mut().into_iter().flatten() {
                if let Some(item) = item.as_object_mut() {
                    item.remove("reacted_to_at");
                }
            }
        }
        if old != new {
            return Err("客户端只能修改已有通知的已读状态".into());
        }
    }
    Ok(())
}

pub fn client_state_only(key: &str) -> bool {
    matches!(
        key,
        "cloud_preferences"
            | "profile_page"
            | "desktop_preferences"
            | "payment_methods"
            | "referral_tracking"
            | "family"
            | "family_notices"
            | "notification_settings"
            | "notifications"
            | "pins"
            | "projects"
            | "auto_top_up"
            | "verified_access"
            | "automations"
            | "installed_plugins"
            | "voice"
            | "onboarding"
            | "browser_settings"
            | "config_bundle"
    )
}

pub fn admin_client_fields(key: &str) -> Vec<crate::ClientField> {
    crate::client_fields(key)
        .into_iter()
        .filter(|field| key != "user_settings" || field.path.first().is_some_and(|p| p == "flags"))
        .collect()
}

pub fn validate_admin_client_change(
    key: &str,
    before: &Value,
    after: &Value,
) -> std::result::Result<(), String> {
    if client_state_only(key) {
        return Err("该数据由客户端或实际业务流程维护，管理端仅提供查询".into());
    }
    let fields = admin_client_fields(key);
    let allowed: Option<Vec<Vec<String>>> = match key {
        "user_settings" | "account_settings" => Some(fields.into_iter().map(|f| f.path).collect()),
        "feature_bootstrap" => Some(fields.into_iter().map(|f| f.path[..2].to_vec()).collect()),
        "trusted_contact" => Some(vec![vec!["enabled".into()]]),
        "conversation_metadata" => Some(vec![
            vec!["default_model_slug".into()],
            vec!["file_attachment_limits".into()],
        ]),
        _ => None,
    };
    if let Some(allowed) = allowed {
        fn strip(value: &mut Value, path: &[String]) {
            let Some((key, tail)) = path.split_first() else {
                return;
            };
            let Some(object) = value.as_object_mut() else {
                return;
            };
            if tail.is_empty() {
                object.remove(key);
            } else if let Some(child) = object.get_mut(key) {
                strip(child, tail);
                if child.as_object().is_some_and(|o| o.is_empty()) {
                    object.remove(key);
                }
            }
        }
        let (mut old, mut new) = (before.clone(), after.clone());
        for path in allowed {
            strip(&mut old, &path);
            strip(&mut new, &path);
        }
        if old != new {
            return Err(
                "不能修改客户端状态或未定义的协议字段，请刷新页面后使用已提供的业务控件".into(),
            );
        }
    }
    Ok(())
}

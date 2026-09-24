//! Shared virtual-account configuration contracts used by both admin and client APIs.
use crate::{Result, Storage, StorageError, VirtualClientState};
use serde_json::{Value, json};

pub struct VirtualConfigSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub default: Value,
}

pub fn virtual_config_specs() -> Vec<VirtualConfigSpec> {
    let entries = [
        (
            "desktop_model_policy",
            "Desktop 推理强度设置",
            "控制原生配置页入口及模型支持的 Ultra 档位可见性；用户选择的档位仍由 Desktop 保存。",
            json!({"reasoning_settings_enabled":true,"ultra_effort_available":true}),
        ),
        (
            "computer_use_policy",
            "浏览器与电脑操控",
            "向 Desktop 提供本服务的功能可用性；安装、浏览器扩展和本机审批继续由客户端维护。",
            json!({"browser_enabled":true,"computer_enabled":true}),
        ),
        (
            "subscription_entitlements",
            "付费套餐权益",
            "每个套餐的模型范围及费用窗口。空模型列表允许全部模型；空金额沿用账号额度，两处均设置时取较低值。",
            json!({"plus":{"models":[],"primary_cost_limit_usd":null,"weekly_cost_limit_usd":null},"pro":{"models":[],"primary_cost_limit_usd":null,"weekly_cost_limit_usd":null},"business":{"models":[],"primary_cost_limit_usd":null,"weekly_cost_limit_usd":null},"enterprise":{"models":[],"primary_cost_limit_usd":null,"weekly_cost_limit_usd":null},"edu":{"models":[],"primary_cost_limit_usd":null,"weekly_cost_limit_usd":null}}),
        ),
        (
            "cloud_preferences",
            "云端偏好",
            "客户端维护的分支命名和差异显示偏好。",
            json!({"branch_format":"codex/{task_id}","git_diff_mode":"unified"}),
        ),
        (
            "subscription_policy",
            "免费层访问策略",
            "无有效付费订阅时的执行权限与独立免费层费用窗口；不会停用登录。",
            json!({"free_access_enabled":false,"primary_cost_limit_usd":0,"weekly_cost_limit_usd":0}),
        ),
        (
            "feature_bootstrap",
            "客户端功能配置",
            "Statsig 功能开关与动态配置；身份由虚拟账号注入，不读取供应账户实验状态。",
            json!({"feature_gates":{},"dynamic_configs":{},"layer_configs":{},"has_updates":true}),
        ),
        (
            "quota",
            "虚拟账号额度",
            "分别设置 5 小时和每周费用上限（美元），任一达到即限制新请求；留空不设限。仅计本账号费用。",
            json!({"primary_cost_limit_usd":null,"weekly_cost_limit_usd":null}),
        ),
        (
            "profile",
            "个人资料",
            "头像及个人简介；身份名称和邮箱在账号表单维护。",
            json!({"picture":null,"bio":""}),
        ),
        (
            "desktop_preferences",
            "旧版协议配置",
            "兼容能力由已实现的协议自动提供，此历史记录不再控制客户端语言和个人页。",
            json!({"localized_interface":true,"profile_enabled":true}),
        ),
        (
            "profile_page",
            "个人信息页",
            "客户端显示名称和用户名留空时使用账号资料；统计来自本账号实际记录。",
            json!({"display_name":null,"username":null,"photo_frame_style":"circle","display_settings":{"show_usage_stats_section":true,"show_activity_graph_section":true,"show_insights_section":true,"show_top_plugins_section":true}}),
        ),
        (
            "account_settings",
            "账号功能设置",
            "客户端工作区设置和功能开关。",
            json!({"beta_settings":{},"beta_settings_str":{},"permissions":[],"usage_limit_increase_request":{"kind":"disabled"}}),
        ),
        (
            "user_settings",
            "功能可用性与用户偏好",
            "功能可用性由服务端管理；个人偏好由客户端保存，在客户端记录中查询。",
            json!({"settings":{},"flags":{}}),
        ),
        (
            "config_bundle",
            "客户端配置包",
            "官方客户端解析的 TOML 配置与要求。",
            json!({"config_toml":{"enterprise_managed":[]},"requirements_toml":{"enterprise_managed":[]}}),
        ),
        (
            "referrals",
            "邀请资格",
            "本虚拟账号的邀请展示配置。",
            json!({"should_show":false}),
        ),
        (
            "referral_tracking",
            "邀请记录",
            "维护此虚拟账号的邀请展示记录；保存记录不会发送邮件。",
            json!({"items":[]}),
        ),
        (
            "payment_methods",
            "支付资料",
            "自行维护的虚拟支付资料，不执行真实扣款。",
            json!({"payment_methods":[]}),
        ),
        (
            "trusted_contact",
            "信任联系人",
            "联系人功能及本地联系人资料。",
            json!({"enabled":false,"contacts":[]}),
        ),
        (
            "family",
            "家庭资料",
            "本地虚拟家庭关系；null 表示尚未配置。",
            Value::Null,
        ),
        (
            "pricing",
            "地区定价配置",
            "以地区代码为键的本系统定价配置，不代表官方报价。",
            json!({}),
        ),
        (
            "gift_credits",
            "赠送资格",
            "本虚拟账号的赠送功能展示配置。",
            json!({"eligible":false}),
        ),
        (
            "notification_settings",
            "通知设置",
            "按 category、options/channel 配置通知渠道。",
            json!({"settings":[]}),
        ),
        (
            "notifications",
            "通知记录",
            "本虚拟账号的通知；修改后向其事件连接发布更新。",
            json!({"items":[],"cursor":null}),
        ),
        (
            "pins",
            "账号置顶记录",
            "本账号置顶项目及会话，使用客户端的 item_type 和资源 ID。",
            json!([]),
        ),
        (
            "first_party",
            "侧栏功能资格",
            "本虚拟账号的 finances 与 health_eligibility 配置。",
            json!({"finances":false,"health_eligibility":{"sidebar_visible":false}}),
        ),
        (
            "projects",
            "账号云项目记录",
            "本账号项目记录，items 每项包含 gizmo 和 conversations。",
            json!({"items":[],"cursor":null}),
        ),
        (
            "system_hints",
            "系统提示",
            "客户端可选提示列表。",
            json!({"system_hints":[]}),
        ),
        (
            "age",
            "年龄资料",
            "自行配置的年龄信息，不等同于官方年龄认证。",
            json!({"is_adult":null,"has_verified_age_or_dob":false}),
        ),
        (
            "models",
            "ChatGPT 模型目录",
            "ChatGPT 页面模型目录；与 Codex 模型目录协议不同。",
            json!({"models":[],"categories":[],"versions":[],"internal_groups":[],"slider_settings":[],"default_model_slug":null}),
        ),
        (
            "conversation_metadata",
            "会话配置",
            "会话默认模型、上传限制、功能限制等元数据。",
            json!({"type":"conversation_detail_metadata","default_model_slug":null,"file_attachment_limits":null,"limits_progress":null,"blocked_features":[]}),
        ),
        (
            "auto_top_up",
            "自动充值展示",
            "本系统的展示配置，不执行供应账户的充值操作。",
            json!({"is_enabled":false,"payment_method":null,"recharge_threshold":null,"recharge_target":null,"recharge_monthly_limit":null,"auto_reload_credit_discount_policy":null}),
        ),
        (
            "discount_offer",
            "优惠展示",
            "本系统优惠资料。",
            json!({"offer":null}),
        ),
        (
            "sites",
            "站点资格",
            "本虚拟账号的站点配置。",
            json!({"enabled":false}),
        ),
        (
            "verified_access",
            "访问资格",
            "本虚拟账号的授权配置。",
            json!({"programs":[]}),
        ),
        (
            "automations",
            "自动化配置",
            "本账号自动化定义；执行状态不能冒充已执行结果。",
            json!({"items":[],"cursor":null}),
        ),
        (
            "beacons",
            "首页公告",
            "本虚拟账号的公告。",
            json!({"beacon_ui_response":null}),
        ),
        (
            "workspace_messages",
            "工作区消息",
            "本虚拟账号的工作区消息列表。",
            json!({"messages":[]}),
        ),
        (
            "installed_plugins",
            "已安装插件",
            "本虚拟账号自身的安装记录。",
            json!({"plugins":[]}),
        ),
        (
            "voice",
            "语音选择",
            "客户端选择的语音名称。",
            json!({"selected":null}),
        ),
        (
            "onboarding",
            "引导状态",
            "客户端完成引导后保存；网页亦可维护。",
            json!({"role":null,"desktop_onboarding_completed_at":null}),
        ),
        (
            "browser_settings",
            "浏览器设置",
            "偏好与站点规则；保存使用版本校验，避免覆盖客户端并发修改。",
            json!({"preferences":{"approval_mode":"always_ask","history_approval_mode":"always_ask","iab_history_approval_mode":"always_ask","download_approval_mode":"always_ask","upload_approval_mode":"always_ask","disable_auto_review":false,"full_cdp_access_enabled":false,"webmcp_enabled":true},"rules":{"origin":{},"download":{},"upload":{},"full_cdp":{}}}),
        ),
    ];
    entries
        .into_iter()
        .map(|(key, label, description, default)| VirtualConfigSpec {
            key,
            label,
            description,
            default,
        })
        .collect()
}

pub fn validate_virtual_config(key: &str, value: &Value) -> std::result::Result<(), String> {
    crate::validate_client_fields(key, value)?;
    let spec = virtual_config_specs()
        .into_iter()
        .find(|s| s.key == key)
        .ok_or_else(|| "未知配置项".to_owned())?;
    if value.to_string().len() > 256 * 1024 {
        return Err("配置超过 256 KiB".into());
    }
    fn shape(expected: &Value, actual: &Value) -> bool {
        match expected {
            Value::Null => true,
            Value::Bool(_) => actual.is_boolean(),
            Value::String(_) => actual.is_string(),
            Value::Number(_) => actual.is_number(),
            Value::Array(_) => actual.is_array(),
            Value::Object(fields) => {
                actual.is_object()
                    && fields
                        .iter()
                        .all(|(k, v)| actual.get(k).is_some_and(|a| shape(v, a)))
            }
        }
    }
    if !shape(&spec.default, value) {
        return Err("配置结构不符合客户端协议，请保留默认字段及其类型".into());
    }
    if key == "subscription_entitlements" {
        for entry in value.as_object().unwrap().values() {
            if !entry["models"].as_array().is_some_and(|a| {
                a.iter().all(|v| {
                    v.as_str()
                        .is_some_and(|s| !s.trim().is_empty() && s.len() <= 256)
                })
            }) || ["primary_cost_limit_usd", "weekly_cost_limit_usd"]
                .iter()
                .any(|key| {
                    !entry[key].is_null()
                        && crate::decimal_units(&entry[key].to_string(), 9).is_none()
                })
            {
                return Err("套餐模型范围或费用额度无效".into());
            }
        }
    }
    crate::validate_virtual_contract(key, value)?;
    if key == "profile_page" {
        for field in ["display_name", "username"] {
            if !value[field].is_null()
                && !value[field]
                    .as_str()
                    .is_some_and(|s| s.len() <= 128 && !s.chars().any(char::is_control))
            {
                return Err("客户端名称或用户名无效".into());
            }
        }
        if !matches!(
            value["photo_frame_style"].as_str(),
            Some("circle" | "scalloped_circle" | "rounded_square" | "oval" | "scalloped_oval")
        ) {
            return Err("请选择有效头像形状".into());
        }
    }
    if key == "referral_tracking" {
        let mut ids = std::collections::HashSet::new();
        for item in value["items"].as_array().unwrap() {
            let text = |key: &str, max: usize| {
                item[key].as_str().filter(|s| {
                    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
                })
            };
            let Some(id) = text("referral_id", 128) else {
                return Err("每条邀请记录必须有有效标识".into());
            };
            if !ids.insert(id)
                || text("program_id", 128).is_none()
                || !text("email", 254).is_some_and(|s| s.contains('@'))
            {
                return Err("邀请记录的标识不能重复，活动标识和邮箱不能为空".into());
            }
            if !matches!(
                item["status"].as_str(),
                Some("pending" | "redeemed" | "expired")
            ) || text("created_at", 64)
                .is_none_or(|s| chrono::DateTime::parse_from_rfc3339(s).is_err())
            {
                return Err("请选择有效的邀请状态和创建时间".into());
            }
            if item.get("can_resend").is_some_and(|v| v != false) {
                return Err("本系统尚未接入邀请邮件发送，不能开启重新发送".into());
            }
            if let Some(link) = item.get("invite_url").filter(|v| !v.is_null())
                && !link.as_str().is_some_and(|s| {
                    url::Url::parse(s).is_ok_and(|u| {
                        matches!(u.scheme(), "http" | "https")
                            && u.username().is_empty()
                            && u.password().is_none()
                    })
                })
            {
                return Err("邀请链接必须是有效的 HTTP(S) 地址".into());
            }
        }
    }
    if matches!(key, "quota" | "subscription_policy")
        && ["primary_cost_limit_usd", "weekly_cost_limit_usd"]
            .iter()
            .any(|key| {
                !value[key].is_null() && crate::decimal_units(&value[key].to_string(), 9).is_none()
            })
    {
        return Err("费用上限必须为非负美元金额，最多九位小数；留空表示不限额".into());
    }
    if key == "quota"
        && value.as_object().is_some_and(|fields| {
            fields.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "primary_cost_limit_usd" | "weekly_cost_limit_usd"
                )
            })
        })
    {
        return Err("额度配置只支持 5 小时和每周费用上限，请刷新页面后保存".into());
    }
    if key == "family" && !value.is_null() && !value.is_object() {
        return Err("家庭资料必须是对象或 null".into());
    }
    if key == "age" && !value["is_adult"].is_null() && !value["is_adult"].is_boolean() {
        return Err("is_adult 必须是布尔值或 null".into());
    }
    if key == "models"
        && !value["models"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["slug"].as_str().is_some_and(|s| !s.is_empty()))
    {
        return Err("每个模型必须有 slug".into());
    }
    if key == "account_settings"
        && (!value["beta_settings"]
            .as_object()
            .unwrap()
            .values()
            .all(Value::is_boolean)
            || !matches!(
                value["usage_limit_increase_request"]["kind"].as_str(),
                Some("disabled" | "custom" | "openai_native")
            ))
    {
        return Err("账号功能开关或额度申请类型无效".into());
    }
    if key == "pricing"
        && !value.as_object().unwrap().iter().all(|(country, v)| {
            country.len() == 2
                && country.bytes().all(|b| b.is_ascii_uppercase())
                && v["country_code"] == country.as_str()
                && v["currency_config"].is_object()
        })
    {
        return Err("地区配置需包含匹配的 country_code 和 currency_config 对象".into());
    }
    if key == "notification_settings"
        && !value["settings"].as_array().unwrap().iter().all(|s| {
            s["category"].is_string()
                && s["options"].as_array().is_some_and(|opts| {
                    opts.iter()
                        .all(|o| o["channel"].is_string() && o["enabled"].is_boolean())
                })
        })
    {
        return Err("通知设置需包含 category 和 options(channel, enabled)".into());
    }
    if key == "notifications"
        && !value["items"].as_array().unwrap().iter().all(|v| {
            v["id"].as_str().is_some_and(|s| !s.is_empty())
                && v["notification_type"].is_string()
                && v["payload"].is_object()
        })
    {
        return Err("每条通知需有 id、notification_type 和 payload 对象".into());
    }
    if key == "projects"
        && !value["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["gizmo"].is_object() && v["conversations"].is_array())
    {
        return Err("项目需有 gizmo 对象和 conversations 数组".into());
    }
    if key == "browser_settings" {
        for (name, v) in value["preferences"].as_object().unwrap() {
            let valid = match name.as_str() {
                "approval_mode" | "download_approval_mode" | "upload_approval_mode" => {
                    matches!(v.as_str(), Some("always_ask" | "never_ask"))
                }
                "history_approval_mode" | "iab_history_approval_mode" => {
                    matches!(v.as_str(), Some("always_ask" | "never_ask" | "disabled"))
                }
                "disable_auto_review" | "full_cdp_access_enabled" | "webmcp_enabled" => {
                    v.is_boolean()
                }
                _ => false,
            };
            if !valid {
                return Err("浏览器偏好值无效".into());
            }
        }
        if !value["rules"].as_object().unwrap().values().all(|r| {
            r.as_object().is_some_and(|r| {
                r.iter().all(|(k, v)| {
                    !k.is_empty() && k.len() <= 2048 && matches!(v.as_str(), Some("allow" | "deny"))
                })
            })
        }) {
            return Err("浏览器规则无效".into());
        }
    }
    Ok(())
}

impl Storage {
    pub async fn create_virtual_conduit(
        &self,
        owner: &str,
        source: &str,
        token: &str,
    ) -> Result<String> {
        let local = format!("c2ct_{}", crate::oauth_secret());
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM virtual_conduits WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO virtual_conduits(token_hash,virtual_account_id,upstream_account_id,upstream_token,expires_at) VALUES(?,?,?,?,?)")
            .bind(crate::hash_token(&local)).bind(owner).bind(source).bind(token).bind(now+900).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(local)
    }
    pub async fn virtual_conduit(
        &self,
        owner: &str,
        source: &str,
        token: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT upstream_token FROM virtual_conduits WHERE token_hash=? AND virtual_account_id=? AND upstream_account_id=? AND expires_at>?")
            .bind(crate::hash_token(token)).bind(owner).bind(source).bind(chrono::Utc::now().timestamp()).fetch_optional(self.pool()).await?)
    }
    pub async fn record_virtual_analytics(&self, owner: &str, events: &[Value]) -> Result<()> {
        let mut tx = self.pool().begin().await?;
        let now = chrono::Utc::now().timestamp_millis();
        for event in events {
            let Some(kind) = event["event_type"].as_str().filter(|s| {
                s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            }) else {
                continue;
            };
            let params = &event["event_params"];
            let mut metadata = json!({"event_type":kind,"received_at_ms":now});
            for key in [
                "thread_id",
                "turn_id",
                "session_id",
                "action",
                "rating",
                "capture_status",
                "plugin_id",
                "plugin_name",
                "model",
                "model_slug",
                "product_client_id",
                "status",
            ] {
                if let Some(v) = params[key]
                    .as_str()
                    .filter(|s| s.len() <= 256 && !s.chars().any(char::is_control))
                {
                    metadata[key] = v.into();
                }
            }
            for key in ["created_at", "completed_at", "captured_diff_bytes"] {
                if let Some(v) = params[key].as_i64().filter(|v| *v >= 0) {
                    metadata[key] = v.into();
                }
            }
            for key in ["skill_id", "skill_name"] {
                if let Some(v) = event[key]
                    .as_str()
                    .filter(|s| s.len() <= 256 && !s.chars().any(char::is_control))
                {
                    metadata[key] = v.into();
                }
            }
            if let Some(v) = params["app_server_client"]["product_client_id"]
                .as_str()
                .filter(|s| s.len() <= 128)
            {
                metadata["product_client_id"] = v.into();
            }
            let id = crate::hash_token(&event.to_string());
            sqlx::query("INSERT OR IGNORE INTO virtual_resources(virtual_account_id,kind,id,value_json,created_at_ms,updated_at_ms) VALUES(?,'analytics',?,?,?,?)")
                .bind(owner).bind(id).bind(metadata.to_string()).bind(now).bind(now).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn virtual_analytics(&self, owner: &str, start: i64, end: i64) -> Result<Vec<Value>> {
        let rows:Vec<String>=sqlx::query_scalar("SELECT value_json FROM virtual_resources WHERE virtual_account_id=? AND kind='analytics' AND created_at_ms>=? AND created_at_ms<? ORDER BY created_at_ms,id")
            .bind(owner).bind(start).bind(end).fetch_all(self.pool()).await?;
        rows.into_iter()
            .map(|s| Ok(serde_json::from_str(&s)?))
            .collect()
    }
    pub async fn virtual_quota(&self, owner: &str) -> Result<Value> {
        self.virtual_quota_at(owner, chrono::Utc::now().timestamp())
            .await
    }
    pub async fn virtual_spending_limited(&self, owner: &str) -> Result<bool> {
        let quota = self.virtual_quota(owner).await?;
        Ok(!quota["rate_limit"]["primary_window"].is_null()
            || !quota["rate_limit"]["secondary_window"].is_null())
    }
    pub(crate) async fn virtual_quota_at(&self, owner: &str, now: i64) -> Result<Value> {
        let account = self
            .virtual_account(owner)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(owner.into()))?;
        let plan = self
            .virtual_plan(&account.plan_id)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(owner.into()))?;
        let billing = self.billing_summary(owner).await?;
        let paid = account.effective_plan_at(now) != "free";
        let rules =
            crate::plan_spending_windows(&plan.config, !paid && account.plan_type != "free")?;
        let mut allowed = self
            .effective_entitlements_at(owner, now)
            .await?
            .execution_enabled;
        let stored_anchor: i64 = sqlx::query_scalar("SELECT COALESCE(unixepoch(subscription_started_at),unixepoch(created_at)) FROM virtual_accounts WHERE id=?")
            .bind(owner).fetch_one(self.pool()).await?;
        let anchor = if stored_anchor > now {
            0
        } else {
            stored_anchor
        };
        let windows = self
            .nested_spending_windows(owner, &rules, anchor, now)
            .await?;
        allowed &= windows.iter().all(|window| window["allowed"] != false);
        // Official Codex names the short window primary. A single outer window
        // remains primary, matching free accounts that only report 30 days.
        let active_inner = windows
            .get(1)
            .filter(|window| !window["started_at"].is_null());
        let primary = active_inner
            .cloned()
            .or_else(|| windows.first().cloned())
            .unwrap_or(Value::Null);
        let secondary = active_inner
            .and_then(|_| windows.first().cloned())
            .unwrap_or(Value::Null);
        Ok(
            json!({"account_id":account.id,"user_id":format!("user-{}",account.id),"plan_type":account.effective_plan_at(now),"rate_limit":{"allowed":allowed,"limit_reached":!allowed,"primary_window":primary,"secondary_window":secondary,"windows":windows},"credits":{"has_credits":false,"unlimited":false,"balance":null},"billing":billing}),
        )
    }
    pub async fn virtual_config(&self, owner: &str, key: &str) -> Result<VirtualClientState> {
        if crate::plan_owned_config(key) {
            return self.virtual_plan_config(owner, key).await;
        }
        let spec = virtual_config_specs()
            .into_iter()
            .find(|s| s.key == key)
            .ok_or(StorageError::InvalidAdminUpdate(
                "Unknown virtual configuration",
            ))?;
        // Materialize defaults once: admin and client read the same persisted record.
        if let Some(saved) = self.virtual_client_state(owner, key).await? {
            return Ok(saved);
        }
        sqlx::query("INSERT OR IGNORE INTO virtual_client_state(virtual_account_id,state_key,value_json,revision,write_origin) SELECT id,?,?,0,'system' FROM virtual_accounts WHERE id=?")
            .bind(key).bind(spec.default.to_string()).bind(owner).execute(self.pool()).await?;
        self.virtual_client_state(owner, key)
            .await?
            .ok_or_else(|| StorageError::AccountNotFound(owner.into()))
    }

    pub async fn update_virtual_config(
        &self,
        owner: &str,
        key: &str,
        value: &Value,
        revision: i64,
    ) -> Result<Option<i64>> {
        self.update_virtual_config_owned(owner, key, value, revision, "system")
            .await
    }

    pub async fn update_virtual_client_config(
        &self,
        owner: &str,
        key: &str,
        value: &Value,
        revision: i64,
    ) -> Result<Option<i64>> {
        let previous = self.virtual_config(owner, key).await?;
        if previous.revision != revision {
            return Ok(None);
        }
        crate::validate_client_change(key, &previous.value, value)
            .map_err(StorageError::Constraint)?;
        self.update_virtual_config_owned(owner, key, value, revision, "client")
            .await
    }
    pub async fn update_virtual_admin_config(
        &self,
        owner: &str,
        key: &str,
        value: &Value,
        revision: i64,
    ) -> Result<Option<i64>> {
        if crate::plan_owned_config(key) {
            return Err(StorageError::Constraint(
                "请在套餐管理中修改套餐配置".into(),
            ));
        }
        if crate::client_state_only(key) {
            return Err(StorageError::Constraint("客户端状态仅供查询".into()));
        }
        let previous = self.virtual_config(owner, key).await?;
        if previous.revision != revision {
            return Ok(None);
        }
        crate::validate_admin_client_change(key, &previous.value, value)
            .map_err(StorageError::Constraint)?;
        if key == "system_hints" {
            let plugins = self.virtual_config(owner, "installed_plugins").await?.value;
            let connectors = self.virtual_resources(owner, "connector_catalog").await?;
            for hint in value["system_hints"].as_array().into_iter().flatten() {
                let target = &hint["resource_id"];
                let found = match hint["hint_kind"].as_str() {
                    Some("plugin") => plugins["plugins"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|p| p["id"] == *target),
                    Some("connector") => connectors.iter().any(|p| p["id"] == *target),
                    _ => true,
                };
                if !found {
                    return Err(StorageError::Constraint(
                        "提示只能关联本账户已有的插件或已读取的连接器目录".into(),
                    ));
                }
            }
        }
        if key == "models" {
            return Err(StorageError::Constraint(
                "模型目录由全局模型管理维护".into(),
            ));
        }
        if key == "conversation_metadata" && !value["default_model_slug"].is_null() {
            let account = self
                .virtual_account(owner)
                .await?
                .ok_or_else(|| StorageError::AccountNotFound(owner.into()))?;
            let models = self
                .available_virtual_models(owner, &account.provider_id)
                .await?;
            if !models
                .iter()
                .any(|model| model.kind == "text" && value["default_model_slug"] == model.model)
            {
                return Err(StorageError::Constraint(
                    "会话默认模型必须存在于账户有权使用的全局目录".into(),
                ));
            }
        }
        self.update_virtual_config_owned(owner, key, value, revision, "admin")
            .await
    }
    async fn update_virtual_config_owned(
        &self,
        owner: &str,
        key: &str,
        value: &Value,
        revision: i64,
        origin: &str,
    ) -> Result<Option<i64>> {
        if crate::plan_owned_config(key) {
            return Err(StorageError::Constraint(
                "请在套餐管理中修改套餐配置".into(),
            ));
        }
        validate_virtual_config(key, value).map_err(StorageError::Constraint)?;
        let previous = self
            .virtual_client_state(owner, key)
            .await?
            .map(|s| s.value)
            .unwrap_or(Value::Null);
        let mut tx = self.pool().begin().await?;
        let changed:Option<i64>=sqlx::query_scalar("UPDATE virtual_client_state SET value_json=?,revision=revision+1,write_origin=?,updated_at_ms=? WHERE virtual_account_id=? AND state_key=? AND revision=? RETURNING revision")
            .bind(value.to_string()).bind(origin).bind(chrono::Utc::now().timestamp_millis()).bind(owner).bind(key).bind(revision).fetch_optional(&mut *tx).await?;
        if changed.is_some() {
            if key == "notifications" {
                for item in value["items"].as_array().into_iter().flatten() {
                    let old = previous["items"]
                        .as_array()
                        .and_then(|a| a.iter().find(|v| v["id"] == item["id"]));
                    if old.is_some_and(|v| {
                        v["payload"] == item["payload"]
                            && v["notification_type"] == item["notification_type"]
                    }) {
                        continue;
                    }
                    let payload = json!({"notification_id":item["id"],"type":item["notification_type"],"payload":item["payload"]});
                    sqlx::query("INSERT INTO virtual_events(virtual_account_id,topic,value_json,created_at_ms) VALUES(?,'app_notifications',?,?)")
                        .bind(owner).bind(payload.to_string()).bind(chrono::Utc::now().timestamp_millis()).execute(&mut *tx).await?;
                }
            }
            let topic = "settings";
            let payload = json!({"type":"setting-updated","payload":{"setting":key}});
            sqlx::query("INSERT INTO virtual_events(virtual_account_id,topic,value_json,created_at_ms) VALUES(?,?,?,?)")
                .bind(owner).bind(topic).bind(payload.to_string()).bind(chrono::Utc::now().timestamp_millis()).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// A Desktop profile edit changes appearance and biography as one revision-checked unit.
    pub async fn update_virtual_profile(
        &self,
        owner: &str,
        page: &VirtualClientState,
        private: &VirtualClientState,
    ) -> Result<bool> {
        validate_virtual_config("profile_page", &page.value).map_err(StorageError::Constraint)?;
        validate_virtual_config("profile", &private.value).map_err(StorageError::Constraint)?;
        let mut tx = self.pool().begin().await?;
        for (key, state) in [("profile_page", page), ("profile", private)] {
            let changed=sqlx::query("UPDATE virtual_client_state SET value_json=?,revision=revision+1,write_origin='client',updated_at_ms=unixepoch()*1000 WHERE virtual_account_id=? AND state_key=? AND revision=?")
                .bind(state.value.to_string()).bind(owner).bind(key).bind(state.revision).execute(&mut *tx).await?.rows_affected();
            if changed != 1 {
                tx.rollback().await?;
                return Ok(false);
            }
            sqlx::query("INSERT INTO virtual_events(virtual_account_id,topic,value_json,created_at_ms) VALUES(?,'settings',?,?)")
                .bind(owner).bind(json!({"type":"setting-updated","payload":{"setting":key}}).to_string()).bind(chrono::Utc::now().timestamp_millis()).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    pub async fn virtual_resources(&self, owner: &str, kind: &str) -> Result<Vec<Value>> {
        let rows:Vec<String>=sqlx::query_scalar("SELECT value_json FROM virtual_resources WHERE virtual_account_id=? AND kind=? ORDER BY updated_at_ms DESC,id")
            .bind(owner).bind(kind).fetch_all(self.pool()).await?;
        rows.into_iter()
            .map(|s| Ok(serde_json::from_str(&s)?))
            .collect()
    }

    pub async fn virtual_resource(
        &self,
        owner: &str,
        kind: &str,
        id: &str,
    ) -> Result<Option<(Option<String>, Value)>> {
        let row:Option<(Option<String>,String)>=sqlx::query_as("SELECT upstream_account_id,value_json FROM virtual_resources WHERE virtual_account_id=? AND kind=? AND id=?")
            .bind(owner).bind(kind).bind(id).fetch_optional(self.pool()).await?;
        row.map(|(source, s)| Ok((source, serde_json::from_str(&s)?)))
            .transpose()
    }

    pub async fn save_virtual_resource(
        &self,
        owner: &str,
        kind: &str,
        id: &str,
        source: Option<&str>,
        value: &Value,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        if kind != "conversation" {
            sqlx::query("INSERT INTO virtual_resources(virtual_account_id,kind,id,upstream_account_id,value_json,created_at_ms,updated_at_ms) VALUES(?,?,?,?,?,?,?) ON CONFLICT(virtual_account_id,kind,id) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms")
                .bind(owner).bind(kind).bind(id).bind(source).bind(value.to_string()).bind(now).bind(now).execute(self.pool()).await?;
            return Ok(());
        }
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let old:Option<String>=sqlx::query_scalar("SELECT value_json FROM virtual_resources WHERE virtual_account_id=? AND kind=? AND id=?").bind(owner).bind(kind).bind(id).fetch_optional(&mut *tx).await?;
        sqlx::query("INSERT INTO virtual_resources(virtual_account_id,kind,id,upstream_account_id,value_json,created_at_ms,updated_at_ms) VALUES(?,?,?,?,?,?,?) ON CONFLICT(virtual_account_id,kind,id) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms")
            .bind(owner).bind(kind).bind(id).bind(source).bind(value.to_string()).bind(now).bind(now).execute(&mut *tx).await?;
        if kind == "conversation" && old.as_deref() != Some(value.to_string().as_str()) {
            let previous: Value = old
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or(Value::Null);
            let completed = matches!(
                value["status"].as_str(),
                Some("completed" | "finished_successfully")
            ) && previous["status"] != value["status"];
            let mut types = vec![if old.is_none() {
                "conversation-created"
            } else {
                "conversation-history-update"
            }];
            if completed {
                types.push("conversation-turn-complete");
            }
            for event_type in types {
                let event = json!({"type":event_type,"payload":{"conversation_id":id}});
                for topic in ["conversations", "alder-conversations"] {
                    sqlx::query("INSERT INTO virtual_events(virtual_account_id,topic,value_json,created_at_ms) VALUES(?,?,?,?)").bind(owner).bind(topic).bind(event.to_string()).bind(now).execute(&mut *tx).await?;
                }
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Public catalog ingestion uses bounded atomic batches, not thousands of
    /// writer transactions. Never changes a resource's original supplier.
    pub async fn save_virtual_connector_catalog(&self, owner: &str, apps: &[Value]) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        for batch in apps.chunks(64) {
            let mut insert = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "INSERT INTO virtual_resources(virtual_account_id,kind,id,value_json,created_at_ms,updated_at_ms) ",
            );
            insert.push_values(batch, |mut row, app| {
                row.push_bind(owner)
                    .push_bind("connector_catalog")
                    .push_bind(app["id"].as_str().unwrap_or(""))
                    .push_bind(app.to_string())
                    .push_bind(now)
                    .push_bind(now);
            });
            insert.push(" ON CONFLICT(virtual_account_id,kind,id) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms");
            insert.build().execute(self.pool()).await?;
        }
        Ok(())
    }

    pub async fn virtual_resource_records(&self, owner: &str, kind: &str) -> Result<Vec<Value>> {
        let rows:Vec<(String,Option<String>,String,i64,i64)>=sqlx::query_as("SELECT id,upstream_account_id,value_json,created_at_ms,updated_at_ms FROM virtual_resources WHERE virtual_account_id=? AND kind=? ORDER BY updated_at_ms DESC,id").bind(owner).bind(kind).fetch_all(self.pool()).await?;
        rows.into_iter().map(|(id,source,text,created,updated)|Ok(json!({"id":id,"owner":owner,"source":source,"value":serde_json::from_str::<Value>(&text)?,"created_at_ms":created,"updated_at_ms":updated}))).collect()
    }

    pub async fn virtual_events(&self, owner: &str, after: i64) -> Result<Vec<Value>> {
        let rows:Vec<(i64,String,String)>=sqlx::query_as("SELECT id,topic,value_json FROM virtual_events WHERE virtual_account_id=? AND id>? ORDER BY id LIMIT 100")
            .bind(owner).bind(after).fetch_all(self.pool()).await?;
        rows.into_iter()
            .map(|(id, topic, s)| {
                Ok(json!({"id":id,"topic":topic,"payload":serde_json::from_str::<Value>(&s)?}))
            })
            .collect()
    }

    pub async fn record_virtual_request(
        &self,
        owner: &str,
        device: &str,
        method: &str,
        path: &str,
        status: u16,
        duration: i64,
    ) -> Result<()> {
        sqlx::query("INSERT INTO virtual_request_logs(virtual_account_id,device_id,method,path,status,duration_ms,created_at_ms) SELECT id,?,?,?,?,?,? FROM virtual_accounts WHERE id=?")
            .bind(device).bind(method).bind(path).bind(i64::from(status)).bind(duration).bind(chrono::Utc::now().timestamp_millis()).bind(owner).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn virtual_request_logs(&self, owner: &str, before: i64) -> Result<Vec<Value>> {
        let rows:Vec<(i64,String,String,String,i64,i64,i64)>=sqlx::query_as("SELECT id,device_id,method,path,status,duration_ms,created_at_ms FROM virtual_request_logs WHERE virtual_account_id=? AND id<? ORDER BY id DESC LIMIT 50")
            .bind(owner).bind(before).fetch_all(self.pool()).await?;
        Ok(rows.into_iter().map(|(id,device,method,path,status,duration,at)|json!({"id":id,"device_id":device,"method":method,"path":path,"status":status,"duration_ms":duration,"created_at_ms":at})).collect())
    }

    pub async fn virtual_request_log_page(
        &self,
        owner: &str,
        page: u32,
        page_size: Option<u32>,
        path: &str,
        method: &str,
        result: &str,
    ) -> Result<Value> {
        let page_size = crate::table_page_size(page_size)?;
        let mut tx = self.pool().begin().await?;
        let predicate = "virtual_account_id=? AND instr(lower(path),lower(?))>0 AND (?='' OR method=?) AND (?='' OR (?='success' AND status>=200 AND status<400) OR (?='failed' AND status>=400))";
        let total: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM virtual_request_logs WHERE {predicate}"
        ))
        .bind(owner)
        .bind(path.trim())
        .bind(method)
        .bind(method)
        .bind(result)
        .bind(result)
        .bind(result)
        .fetch_one(&mut *tx)
        .await?;
        let page = i64::from(page).clamp(1, ((total + page_size - 1) / page_size).max(1));
        let rows: Vec<(i64,String,String,String,i64,i64,i64)> = sqlx::query_as(&format!("SELECT id,device_id,method,path,status,duration_ms,created_at_ms FROM virtual_request_logs WHERE {predicate} ORDER BY id DESC LIMIT ? OFFSET ?"))
            .bind(owner).bind(path.trim()).bind(method).bind(method).bind(result).bind(result).bind(result).bind(page_size).bind((page-1)*page_size)
            .fetch_all(&mut *tx).await?;
        tx.commit().await?;
        let items: Vec<Value> = rows.into_iter().map(|(id,device,method,path,status,duration,at)|json!({"id":id,"device_id":device,"method":method,"path":path,"status":status,"duration_ms":duration,"created_at_ms":at})).collect();
        Ok(json!({"items":items,"total":total,"page":page,"page_size":page_size}))
    }
}

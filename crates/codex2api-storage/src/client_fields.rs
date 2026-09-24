//! Business controls derived from the installed Desktop's kfr/xfr, o8s and j_ readers.
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize)]
pub struct ClientField {
    pub path: Vec<String>,
    pub label: &'static str,
    pub group: &'static str,
    pub kind: &'static str,
    pub description: &'static str,
    pub choices: Vec<(&'static str, &'static str)>,
    pub optional: bool,
}

fn field(
    path: &str,
    label: &'static str,
    group: &'static str,
    kind: &'static str,
    description: &'static str,
) -> ClientField {
    ClientField {
        path: path.split('.').map(str::to_owned).collect(),
        label,
        group,
        kind,
        description,
        choices: Vec::new(),
        optional: true,
    }
}

pub fn client_fields(key: &str) -> Vec<ClientField> {
    let mut fields = Vec::new();
    if key == "user_settings" {
        for (name, label, group, description) in [
            (
                "connector_search_enabled",
                "在搜索中使用连接器",
                "搜索与文件",
                "客户端默认开启；连接器仍需实际连接和授权。",
            ),
            (
                "file_library_auto_reference",
                "自动引用文件库",
                "搜索与文件",
                "允许客户端自动引用本账号文件库中的资料。",
            ),
            (
                "memory_in_search",
                "搜索时使用记忆",
                "搜索与文件",
                "控制客户端搜索记忆偏好。",
            ),
            (
                "instant_answers_enabled",
                "即时回答",
                "回答与编辑器",
                "客户端默认开启。",
            ),
            (
                "task_suggestions_enabled",
                "任务建议",
                "回答与编辑器",
                "客户端默认开启任务建议。",
            ),
            (
                "default_to_study_mode",
                "默认使用学习模式",
                "回答与编辑器",
                "控制新聊天的学习模式偏好。",
            ),
            (
                "show_expanded_code_view",
                "展开代码视图",
                "回答与编辑器",
                "在客户端显示展开的代码内容。",
            ),
            (
                "model_picker_prefers_auto_for_instant_preset",
                "即时预设优先自动选择模型",
                "回答与编辑器",
                "保留客户端对即时预设的模型选择偏好。",
            ),
            (
                "model_picker_persists_ultra_effort",
                "模型选择器滑块中的 Ultra",
                "回答与编辑器",
                "客户端选择是否将 Ultra 显示为模型选择器滑块的最高档位。",
            ),
            (
                "voice_enabled",
                "启用语音界面",
                "语音与听写",
                "客户端默认开启；实际语音执行仍依赖可用服务。",
            ),
            (
                "dictation_enabled",
                "启用听写",
                "语音与听写",
                "客户端默认开启；麦克风仍需系统授权。",
            ),
            (
                "developer_mode",
                "开发者模式",
                "开发与安全",
                "显示客户端开发者功能。",
            ),
            (
                "connector_enforce_csp_in_dev_mode",
                "开发模式下执行连接器 CSP",
                "开发与安全",
                "保持连接器内容安全策略检查。",
            ),
            (
                "enable_device_code_auth",
                "允许设备码登录",
                "开发与安全",
                "客户端设备码登录偏好，不替代实际认证。",
            ),
            (
                "enable_flora_network_access",
                "允许浏览器功能访问网络",
                "开发与安全",
                "保存客户端对应网络访问偏好，保留其他权限检查。",
            ),
            (
                "enable_remote_browser_data",
                "允许远程浏览器数据",
                "开发与安全",
                "保存客户端的远程浏览器数据偏好。",
            ),
            (
                "lockdown_mode_enabled",
                "锁定模式",
                "开发与安全",
                "控制客户端锁定模式偏好。",
            ),
            (
                "precise_location_allowed",
                "允许精确位置",
                "隐私与个性化",
                "保存位置偏好；不会授予操作系统位置权限。",
            ),
            (
                "training_allowed",
                "允许数据改进",
                "隐私与个性化",
                "仅保存本虚拟账号的客户端偏好，不代表供应服务的训练政策已改变。",
            ),
            (
                "bazaar_personalization_enabled",
                "个性化推荐",
                "隐私与个性化",
                "客户端默认开启个性化推荐。",
            ),
            (
                "free_ads_opt_out",
                "退出免费版广告个性化",
                "隐私与个性化",
                "保存客户端的广告退出偏好。",
            ),
            (
                "trusted_contacts_opted_out_at",
                "不再提示信任联系人",
                "隐私与个性化",
                "该版本客户端将此字段读取为布尔值。",
            ),
        ] {
            fields.push(field(
                &format!("settings.{name}"),
                label,
                group,
                "boolean",
                description,
            ));
        }
        let mut theme = field(
            "settings.chat_theme",
            "聊天主题色",
            "外观",
            "select",
            "使用客户端支持的主题色。",
        );
        theme.choices = vec![
            ("default", "默认"),
            ("blue", "蓝色"),
            ("green", "绿色"),
            ("yellow", "黄色"),
            ("pink", "粉色"),
            ("orange", "橙色"),
            ("purple", "紫色"),
            ("black", "黑色"),
        ];
        fields.push(theme);
        let mut effort = field(
            "settings.wingman_thinking_effort",
            "语音思考强度",
            "语音与听写",
            "select",
            "客户端支持即时、中等和高三档。",
        );
        effort.choices = vec![("instant", "即时"), ("medium", "中等"), ("high", "高")];
        fields.push(effort);
        fields.push(field(
            "settings.birthday",
            "生日",
            "个人资料",
            "date",
            "使用年-月-日格式，留空使用客户端默认。",
        ));
        for (path, label, group) in [
            ("settings.accessory_id", "所选配饰", "外观"),
            ("settings.contrast_mode", "对比度模式", "外观"),
            ("settings.voice_main_language", "语音主要语言", "语音与听写"),
            ("settings.voice_mode", "语音模式", "语音与听写"),
        ] {
            fields.push(field(path, label, group, "string", "由客户端选择并保存。"));
        }
        for (name, label, description) in [
            (
                "bazaar_consent_required",
                "推荐功能需要同意",
                "要求客户端显示推荐功能同意流程。",
            ),
            (
                "bazaar_personalization_unavailable",
                "推荐个性化不可用",
                "控制客户端个性化功能的可用状态。",
            ),
            (
                "file_library_enabled_for_registration_country",
                "所在地区可使用文件库",
                "本地配置的界面可用状态，不代表官方地区资格。",
            ),
        ] {
            fields.push(field(
                &format!("flags.{name}"),
                label,
                "界面可用状态",
                "boolean",
                description,
            ));
        }
    } else if key == "account_settings" {
        for (path, label, description) in [
            (
                "member_profiles_enabled",
                "显示成员个人资料",
                "控制当前工作区的个人资料入口。",
            ),
            (
                "admin_work_mode_enabled",
                "允许工作模式",
                "客户端工作模式策略。",
            ),
            (
                "admin_work_local_enabled",
                "允许本地工作",
                "客户端本地执行策略；保留本地权限检查。",
            ),
            (
                "beta_settings.enable_message_feedback",
                "允许消息反馈",
                "控制客户端的消息反馈入口。",
            ),
        ] {
            fields.push(field(path, label, "工作区策略", "boolean", description));
        }
        let mut beta = field(
            "desktop_app_beta_policy",
            "桌面测试版策略",
            "工作区策略",
            "select",
            "客户端支持的四种测试版选择策略。",
        );
        beta.choices = vec![
            ("blocked", "禁止"),
            ("optional_default_off", "允许，默认关闭"),
            ("optional_default_on", "允许，默认开启"),
            ("required", "必须启用"),
        ];
        fields.push(beta);
        let mut gpts = field(
            "allow_third_party_gpts",
            "第三方 GPT 策略",
            "工作区策略",
            "select",
            "控制客户端第三方 GPT 使用策略。",
        );
        gpts.choices = vec![
            ("allow_all", "全部允许"),
            ("allow_none", "全部禁止"),
            ("allow_specific", "仅指定范围"),
        ];
        fields.push(gpts);
        let mut quota = field(
            "usage_limit_increase_request.kind",
            "额度申请入口",
            "额度申请",
            "select",
            "选择是否向用户提供本系统的申请入口。",
        );
        quota.choices = vec![
            ("disabled", "不显示"),
            ("custom", "自定义申请入口"),
            ("openai_native", "客户端原生入口"),
        ];
        quota.optional = false;
        fields.push(quota);
        fields.push(field(
            "usage_limit_increase_request.instructions",
            "申请说明",
            "额度申请",
            "text",
            "自定义入口中显示的说明。",
        ));
        fields.push(field(
            "usage_limit_increase_request.request_url",
            "申请链接",
            "额度申请",
            "url",
            "自定义入口的 HTTP(S) 地址。",
        ));
    } else if key == "desktop_model_policy" {
        for (path, label, description) in [
            (
                "reasoning_settings_enabled",
                "显示推理强度设置",
                "在 Desktop 配置页显示“可用推理强度”；具体级别取决于模型实际能力。",
            ),
            (
                "ultra_effort_available",
                "显示模型支持的 Ultra 档位",
                "允许客户端展示已有模型的 Ultra 能力；不替用户选中该档位或改变审批权限。",
            ),
        ] {
            let mut item = field(path, label, "模型设置入口", "boolean", description);
            item.optional = false;
            fields.push(item);
        }
    } else if key == "computer_use_policy" {
        for (path, label, description) in [
            (
                "browser_enabled",
                "允许浏览器操控",
                "允许客户端使用 Chrome 等浏览器的原生扩展；不会代装扩展或修改审批。",
            ),
            (
                "computer_enabled",
                "允许电脑操控",
                "允许 Desktop 显示本机电脑操控能力；仍需客户端插件、系统权限和本机审批。",
            ),
        ] {
            let mut item = field(path, label, "服务可用性", "boolean", description);
            item.optional = false;
            fields.push(item);
        }
    } else if key == "trusted_contact" {
        let mut enabled = field(
            "enabled",
            "提供信任联系人功能",
            "功能可用性",
            "boolean",
            "控制功能入口；联系人资料由客户端维护。",
        );
        enabled.optional = false;
        fields.push(enabled);
    } else if key == "conversation_metadata" {
        fields.push(field(
            "default_model_slug",
            "默认模型",
            "服务端默认值",
            "string",
            "客户端未指定模型时使用。",
        ));
        fields.push(field(
            "file_attachment_limits.max_size_mb",
            "单个文件大小上限（MB）",
            "上传限制",
            "integer",
            "设置附件限制时需填写此项。",
        ));
        fields.push(field(
            "file_attachment_limits.max_count_per_turn",
            "每轮文件数上限",
            "上传限制",
            "integer",
            "留空使用客户端默认。用量进度由实际记录生成。",
        ));
    } else if key == "feature_bootstrap" {
        for (id, label, description) in [
            (
                "1867347216",
                "个性风格",
                "向原生执行配置发布个性风格功能开关。",
            ),
            (
                "1574672957",
                "自定义指令",
                "向原生执行配置发布自定义指令功能开关。",
            ),
            (
                "2470976080",
                "演示文稿大纲",
                "客户端的演示文稿大纲功能开关。",
            ),
            (
                "423161634",
                "演示文稿大纲入口",
                "控制客户端演示文稿大纲入口的展示。",
            ),
        ] {
            let hash = statsig_hash(id);
            fields.push(field(
                &format!("feature_gates.{hash}.value"),
                label,
                "客户端功能",
                "boolean",
                description,
            ));
        }
    }
    fields
}

pub fn statsig_hash(name: &str) -> String {
    name.bytes()
        .fold(0_u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32))
        .to_string()
}

pub fn validate_client_fields(key: &str, value: &Value) -> std::result::Result<(), String> {
    if key == "conversation_metadata"
        && value["file_attachment_limits"].is_object()
        && value["file_attachment_limits"]["max_size_mb"]
            .as_u64()
            .is_none_or(|n| n == 0)
    {
        return Err("设置上传限制时，请填写单个文件大小上限".into());
    }
    for spec in client_fields(key) {
        let mut entry = Some(value);
        for part in &spec.path {
            entry = entry.and_then(|v| v.get(part));
        }
        let Some(entry) = entry else { continue };
        let valid = match spec.kind {
            "boolean" => entry.is_boolean(),
            "select" => entry
                .as_str()
                .is_some_and(|s| spec.choices.iter().any(|(v, _)| s == *v)),
            "date" => entry
                .as_str()
                .is_some_and(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()),
            "integer" => entry.is_null() || entry.as_u64().is_some_and(|n| n > 0 && n <= 1_000_000),
            "url" => {
                entry.is_null()
                    || entry.as_str().is_some_and(|s| {
                        s.is_empty()
                            || url::Url::parse(s).is_ok_and(|u| {
                                matches!(u.scheme(), "http" | "https")
                                    && u.username().is_empty()
                                    && u.password().is_none()
                            })
                    })
            }
            _ => {
                entry.is_null()
                    || entry.as_str().is_some_and(|s| {
                        s.len() <= 2000 && !s.chars().any(|c| c.is_control() && c != '\n')
                    })
            }
        };
        if !valid {
            return Err(format!("{}的值无效", spec.label));
        }
    }
    Ok(())
}

pub fn prepare_client_config(key: &str, value: &mut Value) {
    if key == "feature_bootstrap" {
        for spec in client_fields(key) {
            let id = &spec.path[1];
            if value["feature_gates"][id]["value"].is_boolean() {
                value["feature_gates"][id]["name"] = id.clone().into();
                value["feature_gates"][id]["rule_id"] = json!("local");
            }
        }
    }
    if key == "conversation_metadata" {
        if value["file_attachment_limits"]
            .as_object()
            .is_some_and(|v| v.is_empty())
        {
            value["file_attachment_limits"] = Value::Null;
        }
        if let Some(limits) = value["file_attachment_limits"].as_object_mut() {
            limits.entry("max_count_per_turn").or_insert(Value::Null);
        }
    }
}

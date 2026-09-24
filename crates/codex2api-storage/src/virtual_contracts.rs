//! Named administration fields compiled into the installed Desktop's wire contracts.
use serde_json::{Value, json};

fn id(value: &mut Value, key: &str) {
    if value[key].as_str().is_none_or(str::is_empty) {
        value[key] = uuid::Uuid::new_v4().to_string().into();
    }
}
pub fn prepare_virtual_contract(key: &str, value: &mut Value) {
    if !value.is_object() {
        return;
    }
    match key {
        "system_hints" => {
            for hint in value["system_hints"].as_array_mut().into_iter().flatten() {
                if !hint.is_object() {
                    continue;
                }
                id(hint, "id");
                let kind = hint["hint_kind"].as_str().unwrap_or("basic");
                let target = hint["resource_id"].as_str().unwrap_or("");
                hint["system_hint"] = match kind {
                    "plugin" => format!("plugin:{target}"),
                    "connector" => format!("connector:{target}"),
                    _ => hint["basic_action"].as_str().unwrap_or("search").to_owned(),
                }
                .into();
                hint["name"] = hint["title"].clone();
                for field in [
                    "aliases",
                    "required_features",
                    "required_models",
                    "required_conversation_modes",
                ] {
                    if hint[field].is_null() {
                        hint[field] = json!([]);
                    }
                }
                if hint["allow_in_temporary_chat"].is_null() {
                    hint["allow_in_temporary_chat"] = true.into();
                }
            }
        }
        "models" => {
            for version in value["versions"].as_array_mut().into_iter().flatten() {
                if !version.is_object() {
                    continue;
                }
                id(version, "id");
                if version["display_text"].is_null() {
                    version["display_text"] = version["name"].clone();
                }
                if version["slugs"].is_null() {
                    version["slugs"] = version.get("models").cloned().unwrap_or(json!([]));
                }
                if version["intelligence_presets"].is_null() {
                    version["intelligence_presets"] = json!([]);
                }
                if let Some(o) = version.as_object_mut() {
                    o.remove("name");
                    o.remove("models");
                }
            }
        }
        "beacons" => {
            let beacon = &mut value["beacon_ui_response"];
            if beacon.is_null() || !beacon.is_object() {
                return;
            }
            id(beacon, "beacon_id");
            if beacon.get("title").is_some() {
                let buttons = beacon["buttons"].as_array().cloned().unwrap_or_default();
                let mut actions = Vec::new();
                for button in buttons {
                    let action = button["action"].as_str().unwrap_or("dismiss");
                    let mut params = json!({"action_enum":action});
                    if action == "open_url" {
                        params["url"] = button["url"].clone();
                    }
                    actions.push(json!({"id":uuid::Uuid::new_v4().to_string(),"text":button["text"],"type":"primary","action_v2":params}));
                }
                *beacon = json!({"beacon_id":beacon["beacon_id"],"beacon_name":beacon["title"],"type":"beacon_ui_response","action_items":actions,"ui_info":{"type":if beacon["presentation"]=="modal"{"beacon_modal_info"}else{"beacon_banner_info"},"title":beacon["title"],"description":beacon["description"]}});
            }
        }
        _ => {}
    }
}

pub fn editable_virtual_contract(key: &str, value: &Value) -> Value {
    let mut value = value.clone();
    if key == "models" {
        prepare_virtual_contract(key, &mut value);
    }
    if key == "system_hints" {
        for hint in value["system_hints"].as_array_mut().into_iter().flatten() {
            if hint["hint_kind"].is_null() {
                let wire = hint["system_hint"].as_str().unwrap_or("").to_owned();
                let (kind, target) = if let Some(id) = wire.strip_prefix("plugin:") {
                    ("plugin", id)
                } else if let Some(id) = wire.strip_prefix("connector:") {
                    ("connector", id)
                } else {
                    ("basic", "")
                };
                hint["hint_kind"] = kind.into();
                hint["resource_id"] = target.into();
            }
            if hint["basic_action"].is_null() {
                hint["basic_action"] = "search".into();
            }
        }
    }
    if key == "beacons" && value["beacon_ui_response"]["type"] == "beacon_ui_response" {
        let b = &value["beacon_ui_response"];
        value = json!({"beacon_ui_response":{"beacon_id":b["beacon_id"],"title":b["ui_info"]["title"],"description":b["ui_info"]["description"],"presentation":if b["ui_info"]["type"]=="beacon_modal_info"{"modal"}else{"banner"},"buttons":b["action_items"].as_array().into_iter().flatten().map(|a|json!({"text":a["text"],"action":a["action_v2"]["action_enum"],"url":a["action_v2"]["url"].as_str().unwrap_or("")})).collect::<Vec<_>>()}});
    }
    value
}

pub fn validate_virtual_contract(key: &str, value: &Value) -> Result<(), String> {
    let text = |v: &Value| {
        v.as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 4096)
    };
    match key {
        "system_hints" => {
            for h in value["system_hints"].as_array().into_iter().flatten() {
                if !text(&h["title"]) || !text(&h["system_hint"]) || !h["description"].is_string() {
                    return Err("提示需要标题、内容和服务端生成的提示标识".into());
                }
                if !text(&h["name"])
                    || [
                        "aliases",
                        "required_features",
                        "required_models",
                        "required_conversation_modes",
                    ]
                    .iter()
                    .any(|key| {
                        !h[key]
                            .as_array()
                            .is_some_and(|a| a.iter().all(Value::is_string))
                    })
                    || !h["allow_in_temporary_chat"].is_boolean()
                {
                    return Err("提示缺少客户端筛选字段，请在管理页重新保存".into());
                }
                if matches!(h["hint_kind"].as_str(), Some("plugin" | "connector"))
                    && !text(&h["resource_id"])
                {
                    return Err("请选择提示关联的插件或连接器".into());
                }
                if h.get("hint_kind")
                    .is_some_and(|v| !matches!(v.as_str(), Some("basic" | "plugin" | "connector")))
                {
                    return Err("提示类型无效".into());
                }
                if h["hint_kind"] == "basic"
                    && !matches!(
                        h["system_hint"].as_str(),
                        Some("search" | "picture_v2" | "tatertot")
                    )
                {
                    return Err("请选择已支持的普通提示动作".into());
                }
            }
        }
        "models" => {
            let models = value["models"].as_array().ok_or("模型目录必须是列表")?;
            let mut slugs = std::collections::HashSet::new();
            let efforts = [
                "standard", "extended", "min", "max", "ultra", "xhigh", "zero",
            ];
            for m in models {
                let slug = m["slug"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or("每个模型需要有效名称")?;
                if !slugs.insert(slug) {
                    return Err("模型名称不能重复".into());
                }
                for k in ["title", "description"] {
                    if !m[k].is_null() && !m[k].is_string() {
                        return Err("模型标题和说明必须是文本".into());
                    }
                }
                if m.get("default_thinking_effort").is_some_and(|v| {
                    !v.is_null() && !v.as_str().is_some_and(|s| efforts.contains(&s))
                }) {
                    return Err("默认思考强度无效".into());
                }
                for e in m["thinking_efforts"].as_array().into_iter().flatten() {
                    if !e["thinking_effort"]
                        .as_str()
                        .is_some_and(|s| efforts.contains(&s))
                    {
                        return Err("思考强度无效".into());
                    }
                }
                if m.get("enabled_tools")
                    .is_some_and(|v| !v.as_array().is_some_and(|a| a.iter().all(&text)))
                {
                    return Err("工具名称无效".into());
                }
                if let Some(a) = m
                    .pointer("/product_features/attachments")
                    .filter(|v| !v.is_null())
                {
                    if !matches!(
                        a["type"].as_str(),
                        Some(
                            "code_interpreter"
                                | "multimodal"
                                | "retrieval"
                                | "context_connector"
                                | "multimodal_audio"
                        )
                    ) {
                        return Err("附件类型无效".into());
                    }
                    for k in ["accepted_mime_types", "image_mime_types"] {
                        if a.get(k).is_some_and(|v| {
                            !v.is_null() && !v.as_array().is_some_and(|a| a.iter().all(&text))
                        }) {
                            return Err("附件 MIME 类型必须是文本列表".into());
                        }
                    }
                }
            }
            let exists = |v: &Value| v.as_str().is_some_and(|s| slugs.contains(s));
            if !value["default_model_slug"].is_null() && !exists(&value["default_model_slug"]) {
                return Err("默认模型必须在目录中".into());
            }
            let mut ids = std::collections::HashSet::new();
            for version in value["versions"].as_array().into_iter().flatten() {
                if !text(&version["id"])
                    || !ids.insert(version["id"].as_str())
                    || !text(&version["display_text"])
                {
                    return Err("模型版本需要名称和不重复的自动标识".into());
                }
                let refs = version["slugs"]
                    .as_array()
                    .filter(|a| !a.is_empty() && a.iter().all(exists))
                    .ok_or("模型版本必须关联目录中的模型")?;
                let presets = version["intelligence_presets"]
                    .as_array()
                    .ok_or("模型版本档位必须是列表")?;
                for p in presets {
                    if !exists(&p["model_slug"])
                        || !refs.contains(&p["model_slug"])
                        || !matches!(
                            p["lane"].as_str(),
                            Some("auto" | "instant" | "thinking" | "thinking_mini" | "pro")
                        )
                        || p.get("thinking_effort").is_some_and(|v| {
                            !v.is_null() && !v.as_str().is_some_and(|s| efforts.contains(&s))
                        })
                    {
                        return Err("版本档位、模型引用或思考强度无效".into());
                    }
                }
            }
            for c in value["categories"].as_array().into_iter().flatten() {
                if !exists(&c["default_model"]) {
                    return Err("分类默认模型必须在目录中".into());
                }
            }
            for group in value["internal_groups"].as_array().into_iter().flatten() {
                if !group["model_ids"]
                    .as_array()
                    .is_some_and(|a| a.iter().all(exists))
                {
                    return Err("模型组引用不存在的模型".into());
                }
            }
            for slider in value["slider_settings"].as_array().into_iter().flatten() {
                if !exists(&slider["model_slug"])
                    || !slider["thinking_effort"]
                        .as_str()
                        .is_some_and(|s| efforts.contains(&s))
                {
                    return Err("滑杆模型或思考强度无效".into());
                }
            }
        }
        "beacons" => {
            let b = &value["beacon_ui_response"];
            if b.is_null() {
                return Ok(());
            }
            if b["type"] != "beacon_ui_response"
                || !text(&b["beacon_id"])
                || !text(&b["beacon_name"])
                || !text(&b["ui_info"]["title"])
                || !b["ui_info"]["description"].is_string()
                || !matches!(
                    b["ui_info"]["type"].as_str(),
                    Some("beacon_banner_info" | "beacon_modal_info")
                )
            {
                return Err("公告结构无效，请使用公告标题、正文和展示形式控件重新保存".into());
            }
            for a in b["action_items"].as_array().ok_or("公告按钮必须是列表")? {
                if !text(&a["id"]) || !text(&a["text"]) {
                    return Err("请填写公告按钮文字".into());
                }
                match a["action_v2"]["action_enum"].as_str() {
                    Some("dismiss") => {}
                    Some("open_url") => {
                        if !a["action_v2"]["url"].as_str().is_some_and(|s| {
                            url::Url::parse(s).is_ok_and(|u| {
                                matches!(u.scheme(), "http" | "https")
                                    && u.username().is_empty()
                                    && u.password().is_none()
                            })
                        }) {
                            return Err("公告按钮链接必须是 HTTP(S) 地址".into());
                        }
                    }
                    _ => return Err("只支持关闭公告或打开链接".into()),
                }
            }
        }
        _ => {}
    }
    Ok(())
}

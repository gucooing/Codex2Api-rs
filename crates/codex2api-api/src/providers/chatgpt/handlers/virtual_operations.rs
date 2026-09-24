use axum::{
    Json,
    body::Bytes,
    extract::{Extension, Path, State},
    http::{HeaderMap, Method, StatusCode},
    response::Response,
};
use codex2api_storage::VirtualAccess;
use serde_json::{Value, json};

pub(crate) async fn unavailable_operation(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Extension(operation): Extension<&'static str>,
    Path(params): Path<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> crate::Result<Response> {
    let body = if body.is_empty() {
        json!({})
    } else {
        codex2api_upstream::decode_body(&body, &headers)?
    };
    let plugin = operation.starts_with("plugin_");
    let resource = params
        .get("plugin_id")
        .map(String::as_str)
        .or(body["jawbone_id"].as_str())
        .or(body["automation_id"].as_str());
    if let Some(id) = resource {
        if id.len() > 256 {
            return Err(crate::ApiError::bad_request("Invalid resource ID."));
        }
        if !plugin {
            let saved = state
                .storage
                .virtual_config(&access.virtual_account_id, "automations")
                .await?;
            if !saved.value["items"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|a| a["id"] == id)
            {
                return Err(super::virtual_data::not_found());
            }
        }
    }
    state.storage.save_virtual_resource(&access.virtual_account_id,if plugin{"plugin_operation"}else{"automation_operation"},&uuid::Uuid::new_v4().to_string(),None,&json!({"operation":operation,"resource_id":resource,"status":"executor_unavailable","created_at_ms":chrono::Utc::now().timestamp_millis()})).await?;
    Err(crate::ApiError::openai(
        StatusCode::NOT_IMPLEMENTED,
        "api_error",
        if plugin {
            "An account-isolated cloud plugin executor is not implemented; no supplier installation was changed."
        } else {
            "An account-isolated cloud automation scheduler is not implemented; no task was scheduled."
        },
        Some(if plugin {
            "cloud_plugin_executor_unavailable"
        } else {
            "cloud_automation_executor_unavailable"
        }),
    ))
}

pub(crate) async fn cloud_preferences_schema(
    Extension(_access): Extension<VirtualAccess>,
) -> crate::Result<Response> {
    Ok(crate::providers::chatgpt::identity::json_response(
        json!({"branch_format_max_length":128,"branch_format_special_values":[{"value":"{task_id}","example":"task-123","char_count":36}]}),
    ))
}
pub(crate) async fn cloud_preferences_patch(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Json(body): Json<Value>,
) -> crate::Result<Response> {
    let fields = body
        .as_object()
        .ok_or_else(|| crate::ApiError::bad_request("Expected preferences."))?;
    let saved = state
        .storage
        .virtual_config(&access.virtual_account_id, "cloud_preferences")
        .await?;
    let mut value = saved.value;
    for (key, v) in fields {
        match key.as_str() {
            "git_diff_mode" if matches!(v.as_str(), Some("unified" | "split")) => {}
            "branch_format"
                if v.as_str().is_some_and(|s| {
                    let rendered = s.replace("{task_id}", &"x".repeat(36));
                    s.contains("{task_id}")
                        && rendered.len() <= 128
                        && !rendered.starts_with('/')
                        && rendered
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"./-_".contains(&b))
                        && !rendered.contains("..")
                        && !rendered.ends_with('/')
                }) => {}
            _ => return Err(crate::ApiError::bad_request("Invalid cloud preference.")),
        }
        value[key] = v.clone();
    }
    super::virtual_data::save_config(&state, &access, "cloud_preferences", &value, saved.revision)
        .await?;
    Ok(crate::providers::chatgpt::identity::json_response(value))
}

pub(crate) async fn pin(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Path((kind, id)): Path<(String, String)>,
    method: Method,
) -> crate::Result<Response> {
    let owns = match kind.as_str() {
        "conversation" => state
            .storage
            .virtual_resource(&access.virtual_account_id, "conversation", &id)
            .await?
            .is_some(),
        "project" => state
            .storage
            .virtual_config(&access.virtual_account_id, "projects")
            .await?
            .value["items"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v["gizmo"]["id"] == id),
        _ => return Err(crate::ApiError::bad_request("Invalid pin type.")),
    };
    if !owns {
        return Err(super::virtual_data::not_found());
    }
    let saved = state
        .storage
        .virtual_config(&access.virtual_account_id, "pins")
        .await?;
    let mut value = saved.value;
    let items = value
        .as_array_mut()
        .ok_or_else(|| crate::ApiError::internal("Invalid pins record."))?;
    items.retain(|v| !(v["item_type"] == kind && v["item_id"] == id));
    if method == Method::POST {
        items.push(json!({"id":uuid::Uuid::new_v4().to_string(),"item_type":kind,"item_id":id,"created_at":chrono::Utc::now().to_rfc3339()}));
    }
    super::virtual_data::save_config(&state, &access, "pins", &value, saved.revision).await?;
    Ok(crate::providers::chatgpt::identity::json_response(
        json!({"success":true}),
    ))
}

pub(crate) async fn conversation(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    Path(id): Path<String>,
    headers: HeaderMap,
    method: Method,
    body: Bytes,
) -> crate::Result<Response> {
    let (source, mut summary) = state
        .storage
        .virtual_resource(&access.virtual_account_id, "conversation", &id)
        .await?
        .ok_or_else(super::virtual_data::not_found)?;
    let patch = if method == Method::PATCH {
        codex2api_upstream::decode_body(&body, &headers)?
    } else {
        json!({})
    };
    if method == Method::PATCH {
        let fields = patch
            .as_object()
            .ok_or_else(|| crate::ApiError::bad_request("Expected conversation update."))?;
        for (key, value) in fields {
            if !match key.as_str() {
                "title" => value.as_str().is_some_and(|s| s.len() <= 4096),
                "is_archived" | "is_starred" | "is_visible" => value.is_boolean(),
                _ => false,
            } {
                return Err(crate::ApiError::bad_request(
                    "Unsupported conversation update.",
                ));
            }
        }
    }
    if source.is_none() || source != access.account_id {
        if method == Method::GET
            && let Some((_, detail)) = state
                .storage
                .virtual_resource(&access.virtual_account_id, "conversation_detail", &id)
                .await?
        {
            return Ok(crate::providers::chatgpt::identity::json_response(detail));
        }
        return Err(crate::ApiError::openai(
            StatusCode::CONFLICT,
            "api_error",
            "This conversation belongs to the previous execution account; its saved history remains available.",
            Some("conversation_source_unavailable"),
        ));
    }
    let (_, ctx) = crate::providers::chatgpt::access::resolve_supplier(
        &state,
        &headers,
        Extension(access.clone()),
    )
    .await?;
    let upstream = state.upstream.get(&ctx.account.id).await?;
    let response = upstream
        .forward_raw_chatgpt(
            method.clone(),
            &format!("/backend-api/conversation/{id}"),
            None,
            body,
            headers,
        )
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Ok(crate::response::forward_response(
            status,
            response.headers().clone(),
            axum::body::Body::from_stream(response.bytes_stream()),
        ));
    }
    let mut value: Value = if status == StatusCode::NO_CONTENT {
        Value::Null
    } else {
        response
            .json()
            .await
            .map_err(|e| crate::ApiError::internal(e.to_string()))?
    };
    if let Some(account) = state
        .storage
        .virtual_account(&access.virtual_account_id)
        .await?
    {
        crate::providers::chatgpt::identity::mask(&mut value, &account, &ctx.account);
    }
    if method == Method::GET {
        if value["conversation_id"]
            .as_str()
            .or(value["id"].as_str())
            .is_some_and(|v| v != id)
        {
            return Err(crate::ApiError::internal("Conversation ID mismatch."));
        }
        state
            .storage
            .save_virtual_resource(
                &access.virtual_account_id,
                "conversation_detail",
                &id,
                source.as_deref(),
                &value,
            )
            .await?;
        for key in [
            "title",
            "create_time",
            "update_time",
            "is_archived",
            "is_starred",
            "is_visible",
            "conversation_origin",
        ] {
            if let Some(v) = value.get(key) {
                summary[key] = v.clone();
            }
        }
    } else {
        if value.get("success") == Some(&Value::Bool(false)) {
            return Ok(crate::providers::chatgpt::identity::json_response(value));
        }
        for (key, v) in patch.as_object().unwrap() {
            summary[key] = v.clone();
        }
        summary["update_time"] = chrono::Utc::now().timestamp().into();
        if let Some((_, mut detail)) = state
            .storage
            .virtual_resource(&access.virtual_account_id, "conversation_detail", &id)
            .await?
        {
            for (key, v) in patch.as_object().unwrap() {
                detail[key] = v.clone();
            }
            state
                .storage
                .save_virtual_resource(
                    &access.virtual_account_id,
                    "conversation_detail",
                    &id,
                    source.as_deref(),
                    &detail,
                )
                .await?;
        }
    }
    state
        .storage
        .save_virtual_resource(
            &access.virtual_account_id,
            "conversation",
            &id,
            source.as_deref(),
            &summary,
        )
        .await?;
    let mut out = crate::providers::chatgpt::identity::json_response(value);
    *out.status_mut() = status;
    Ok(out)
}

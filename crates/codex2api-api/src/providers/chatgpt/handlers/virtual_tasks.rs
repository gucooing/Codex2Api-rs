use axum::{
    body::Bytes,
    extract::Extension,
    http::{HeaderMap, Uri},
    response::Response,
};
use codex2api_storage::{VirtualAccess, VirtualAccount};
use codex2api_upstream::BackendEndpoint as E;
use serde_json::{Value, json};
use std::collections::HashMap;

async fn owned(
    state: &crate::ApiState,
    account: &VirtualAccount,
    id: &str,
    execution: bool,
) -> crate::Result<Value> {
    let (source, value) = state
        .storage
        .virtual_resource(&account.id, "task", id)
        .await?
        .ok_or_else(super::virtual_data::not_found)?;
    if execution
        && (source.is_none()
            || source
                != state
                    .storage
                    .execution_route(&account.id, "chatgpt")
                    .await?
                    .and_then(|r| r.supplier_account_id))
    {
        return Err(super::virtual_data::not_found());
    }
    Ok(value)
}

async fn save(
    state: &crate::ApiState,
    account: &VirtualAccount,
    source: &str,
    value: &Value,
) -> crate::Result<()> {
    let id = value["task"]["id"]
        .as_str()
        .or(value["id"].as_str())
        .ok_or_else(|| crate::ApiError::internal("Task response has no ID."))?;
    state
        .storage
        .save_virtual_resource(&account.id, "task", id, Some(source), value)
        .await?;
    for key in [
        "current_user_turn",
        "current_assistant_turn",
        "current_diff_task_turn",
    ] {
        if let Some(turn) = value[key]["id"].as_str() {
            state
                .storage
                .save_virtual_resource(
                    &account.id,
                    "task_turn",
                    &format!("{id}:{turn}"),
                    Some(source),
                    &value[key],
                )
                .await?;
        }
    }
    Ok(())
}

pub(crate) struct TaskRequest {
    pub endpoint: E,
    pub params: HashMap<String, String>,
    pub uri: Uri,
    pub headers: HeaderMap,
    pub body: Bytes,
}

pub(crate) async fn forward(
    state: &crate::ApiState,
    account: &VirtualAccount,
    access: VirtualAccess,
    request: TaskRequest,
) -> crate::Result<Response> {
    let TaskRequest {
        endpoint,
        mut params,
        uri,
        mut headers,
        body,
    } = request;
    let mut value = if body.is_empty() || endpoint.method() == axum::http::Method::GET {
        json!({})
    } else {
        codex2api_upstream::decode_body(&body, &headers)?
    };
    if endpoint == E::CreateTask {
        if value
            .get("metadata")
            .is_some_and(|v| !v.is_null() && !v.is_object())
        {
            return Err(crate::ApiError::bad_request(
                "Task metadata must be an object.",
            ));
        }
        for key in ["account_id", "conversation_owner_id"] {
            if value
                .get(key)
                .is_some_and(|v| !v.is_null() && v.as_str() != Some(&account.id))
            {
                return Err(crate::ApiError::bad_request("SupplierAccount mismatch."));
            }
        }
        if let Some(follow) = value.get("follow_up") {
            let id = follow["task_id"]
                .as_str()
                .ok_or_else(|| crate::ApiError::bad_request("follow_up.task_id is required."))?;
            owned(state, account, id, true).await?;
            let turn = follow["turn_id"]
                .as_str()
                .ok_or_else(|| crate::ApiError::bad_request("follow_up.turn_id is required."))?;
            if state
                .storage
                .virtual_resource(&account.id, "task_turn", &format!("{id}:{turn}"))
                .await?
                .is_none()
            {
                return Err(super::virtual_data::not_found());
            }
        }
        if let Some(id) = value["task_id"].as_str() {
            owned(state, account, id, true).await?;
        }
        crate::providers::chatgpt::access::check_unpriced_execution(&state.storage, &account.id)
            .await?;
    } else if endpoint != E::Tasks {
        let id = params
            .get("task_id")
            .ok_or_else(super::virtual_data::not_found)?;
        let saved = owned(state, account, id, false).await?;
        let (source, _) = state
            .storage
            .virtual_resource(&account.id, "task", id)
            .await?
            .unwrap();
        if source.is_none()
            || source
                != state
                    .storage
                    .execution_route(&account.id, "chatgpt")
                    .await?
                    .and_then(|r| r.supplier_account_id)
        {
            if endpoint == E::Task {
                return Ok(crate::providers::chatgpt::identity::json_response(saved));
            }
            let key = cache_key(endpoint, &params);
            if matches!(
                endpoint,
                E::TaskTurns | E::TaskTurn | E::TaskLogs | E::SiblingTurns
            ) && let Some((_, cached)) = state
                .storage
                .virtual_resource(&account.id, "task_read", &key)
                .await?
            {
                return Ok(crate::providers::chatgpt::identity::json_response(cached));
            }
            return Err(super::virtual_data::not_found());
        }
        if let Some(turn) = params.get("turn_id")
            && state
                .storage
                .virtual_resource(&account.id, "task_turn", &format!("{id}:{turn}"))
                .await?
                .is_none()
        {
            return Err(super::virtual_data::not_found());
        }
    }
    if endpoint == E::Tasks {
        let query: HashMap<_, _> =
            url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes())
                .into_owned()
                .collect();
        let offset = query
            .get("cursor")
            .map(|v| v.parse::<usize>())
            .transpose()
            .map_err(|_| crate::ApiError::bad_request("Invalid cursor."))?
            .unwrap_or(0);
        let limit = query
            .get("limit")
            .map(|v| v.parse::<usize>())
            .transpose()
            .map_err(|_| crate::ApiError::bad_request("Invalid limit."))?
            .unwrap_or(20)
            .clamp(1, 100);
        let filter = query
            .get("task_filter")
            .map(String::as_str)
            .unwrap_or("all");
        if !matches!(filter, "all" | "current" | "archived") {
            return Err(crate::ApiError::bad_request("Invalid task filter."));
        }
        let mut rows = state.storage.virtual_resources(&account.id, "task").await?;
        // Refresh only IDs already owned by this account, never the supplier's list.
        for row in &mut rows {
            let task = row.get("task").unwrap_or(row);
            let Some(id) = task["id"].as_str().map(str::to_owned) else {
                continue;
            };
            let (source, _) = state
                .storage
                .virtual_resource(&account.id, "task", &id)
                .await?
                .unwrap();
            if source.is_some()
                && source
                    == state
                        .storage
                        .execution_route(&account.id, "chatgpt")
                        .await?
                        .and_then(|r| r.supplier_account_id)
            {
                let (_, ctx) = crate::providers::chatgpt::access::resolve_supplier(
                    state,
                    &headers,
                    Extension(access.clone()),
                )
                .await?;
                let client = state.upstream.get(&ctx.account.id).await?;
                params.insert("task_id".into(), id.clone());
                let response = client
                    .forward_backend(E::Task, &params, None, Bytes::new(), headers.clone())
                    .await?;
                if !response.status().is_success() {
                    return Ok(crate::response::forward_response(
                        response.status(),
                        response.headers().clone(),
                        axum::body::Body::from_stream(response.bytes_stream()),
                    ));
                }
                let mut updated: Value = response
                    .json()
                    .await
                    .map_err(|e| crate::ApiError::internal(e.to_string()))?;
                if updated["task"]["id"].as_str().or(updated["id"].as_str()) != Some(id.as_str()) {
                    return Err(crate::ApiError::internal("Task ID mismatch."));
                }
                crate::providers::chatgpt::identity::mask(&mut updated, account, &ctx.account);
                save(state, account, &ctx.account.id, &updated).await?;
                *row = updated;
            }
        }
        rows.retain(|v| {
            let t = v.get("task").unwrap_or(v);
            (filter == "all" || (t["archived"] == true) == (filter == "archived"))
                && query
                    .get("environment_id")
                    .is_none_or(|id| t["environment_id"].as_str() == Some(id))
        });
        rows.sort_by(|a, b| {
            time(&b.get("task").unwrap_or(b)["updated_at"])
                .total_cmp(&time(&a.get("task").unwrap_or(a)["updated_at"]))
                .then_with(|| {
                    a.get("task").unwrap_or(a)["id"]
                        .as_str()
                        .cmp(&b.get("task").unwrap_or(b)["id"].as_str())
                })
        });
        let total = rows.len();
        let items: Vec<_> = rows
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|v| {
                let mut task = v.get("task").unwrap_or(&v).clone();
                if let Some(status) = v["current_assistant_turn"]["turn_status"].as_str() {
                    if task["task_status_display"].is_null() {
                        task["task_status_display"] = json!({});
                    }
                    if task["task_status_display"]["latest_turn_status_display"].is_null() {
                        task["task_status_display"]["latest_turn_status_display"] = json!({});
                    }
                    task["task_status_display"]["latest_turn_status_display"]["turn_status"] =
                        status.into();
                }
                task
            })
            .collect();
        return Ok(crate::providers::chatgpt::identity::json_response(
            json!({"items":items,"cursor":(offset.saturating_add(limit)<total).then(||(offset+limit).to_string())}),
        ));
    }
    let (credential, ctx) =
        crate::providers::chatgpt::access::resolve_supplier(state, &headers, Extension(access))
            .await?;
    let task_model = if endpoint == E::CreateTask {
        match execution_model(&state.storage, &account.id, &ctx.account.id, &value).await {
            Ok(model) => Some(model),
            Err(error) => {
                state.storage.save_virtual_resource(&account.id,"task_operation",&uuid::Uuid::new_v4().to_string(),Some(&ctx.account.id),&json!({"task_id":value.pointer("/follow_up/task_id").or(value.get("task_id")),"operation":if value.get("follow_up").is_some(){"follow_up"}else{"create"},"status":"model_unavailable","reason":"An explicit model or an owned model from the same supplier execution is required.","created_at_ms":chrono::Utc::now().timestamp_millis()})).await?;
                return Err(error);
            }
        }
    } else {
        None
    };
    let body = if let Some(model) = &task_model {
        if value["metadata"].is_null() {
            value["metadata"] = json!({});
        }
        value["metadata"]["model_slug"] = model.clone().into();
        headers.remove("content-encoding");
        headers.remove("content-length");
        Bytes::from(value.to_string())
    } else {
        body
    };
    let mut log = if endpoint == E::CreateTask {
        Some(
            crate::execution::ExecutionContext::new(
                state.storage.clone(),
                &ctx.account,
                &credential.id,
                &credential.name,
                "/backend-api/wham/tasks",
                "http",
            )
            .start(
                codex2api_upstream::RequestMetadata {
                    model: task_model.clone(),
                    ..Default::default()
                },
                std::time::Instant::now(),
                chrono::Utc::now().timestamp_millis(),
            )
            .await?,
        )
    } else {
        None
    };
    let client = state.upstream.get(&ctx.account.id).await?;
    let response = match client
        .forward_backend(endpoint, &params, uri.query(), body, headers)
        .await
    {
        Ok(response) => response,
        Err(error) => {
            if let Some(log) = &mut log {
                log.upstream_failure(&error);
            }
            return Err(error.into());
        }
    };
    let status = response.status();
    if let Some(log) = &mut log {
        log.http_status(status.as_u16());
        log.response_headers(response.headers());
        if status.is_success() {
            log.finish("submitted");
        }
    }
    if !status.is_success() {
        let headers = response.headers().clone();
        let body = if let Some(log) = log {
            log.wrap(response)
        } else {
            axum::body::Body::from_stream(response.bytes_stream())
        };
        return Ok(crate::response::forward_response(status, headers, body));
    }
    let mut result: Value = if status == axum::http::StatusCode::NO_CONTENT {
        Value::Null
    } else {
        response
            .json()
            .await
            .map_err(|e| crate::ApiError::internal(e.to_string()))?
    };
    crate::providers::chatgpt::identity::mask(&mut result, account, &ctx.account);
    if endpoint == E::Task
        && result["task"]["id"].as_str().or(result["id"].as_str())
            != params.get("task_id").map(String::as_str)
    {
        return Err(crate::ApiError::internal("Task ID mismatch."));
    }
    if matches!(endpoint, E::CreateTask | E::Task) {
        save(state, account, &ctx.account.id, &result).await?;
    }
    if let Some(model) = task_model {
        let id = result["task"]["id"]
            .as_str()
            .or(result["id"].as_str())
            .ok_or_else(|| crate::ApiError::internal("Task response has no ID."))?;
        state
            .storage
            .save_virtual_resource(
                &account.id,
                "task_execution",
                id,
                Some(&ctx.account.id),
                &json!({"task_id":id,"model":model}),
            )
            .await?;
    }
    if let Some(id) = params.get("task_id") {
        state
            .storage
            .save_virtual_resource(
                &account.id,
                "task_read",
                &cache_key(endpoint, &params),
                Some(&ctx.account.id),
                &result,
            )
            .await?;
        if endpoint == E::TaskTurns {
            for turn in result
                .as_array()
                .or(result["turns"].as_array())
                .or(result["items"].as_array())
                .into_iter()
                .flatten()
            {
                if let Some(tid) = turn["id"].as_str() {
                    state
                        .storage
                        .save_virtual_resource(
                            &account.id,
                            "task_turn",
                            &format!("{id}:{tid}"),
                            Some(&ctx.account.id),
                            turn,
                        )
                        .await?;
                }
            }
        }
        if matches!(endpoint, E::CancelTask | E::ArchiveTask) {
            // Read actual post-operation state; do not fabricate completion from a 2xx acknowledgement.
            let response = client
                .forward_backend(E::Task, &params, None, Bytes::new(), HeaderMap::new())
                .await?;
            if response.status().is_success() {
                let mut v: Value = response
                    .json()
                    .await
                    .map_err(|e| crate::ApiError::internal(e.to_string()))?;
                crate::providers::chatgpt::identity::mask(&mut v, account, &ctx.account);
                save(state, account, &ctx.account.id, &v).await?;
            }
        }
    }
    let mut out = crate::providers::chatgpt::identity::json_response(result);
    *out.status_mut() = status;
    Ok(out)
}

async fn execution_model(
    storage: &codex2api_storage::Storage,
    owner: &str,
    supplier: &str,
    request: &Value,
) -> crate::Result<String> {
    if let Some(model) = request
        .pointer("/metadata/model_slug")
        .filter(|v| !v.is_null())
    {
        return model
            .as_str()
            .filter(|m| codex2api_core::valid_model(m))
            .map(str::to_owned)
            .ok_or_else(|| crate::ApiError::bad_request("Invalid task model."));
    }
    if let Some(id) = request
        .pointer("/follow_up/task_id")
        .or(request.get("task_id"))
        .and_then(Value::as_str)
        && let Some((source, value)) = storage
            .virtual_resource(owner, "task_execution", id)
            .await?
        && source.as_deref() == Some(supplier)
        && let Some(model) = value["model"]
            .as_str()
            .filter(|m| codex2api_core::valid_model(m))
    {
        return Ok(model.to_owned());
    }
    Err(crate::ApiError::openai(
        axum::http::StatusCode::NOT_IMPLEMENTED,
        "api_error",
        "This task requires an explicit model or a recorded model from its own previous execution.",
        Some("task_model_unavailable"),
    ))
}
fn time(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| {
            value
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.timestamp_millis() as f64 / 1000.0)
        })
        .unwrap_or(0.0)
}
fn cache_key(endpoint: E, params: &HashMap<String, String>) -> String {
    format!(
        "{:?}:{}:{}",
        endpoint,
        params.get("task_id").map(String::as_str).unwrap_or(""),
        params.get("turn_id").map(String::as_str).unwrap_or("")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn task_models_inherit_only_owned_same_supplier_execution_and_accept_explicit_selection()
    {
        let dir = tempfile::tempdir().unwrap();
        let storage = codex2api_storage::Storage::open(dir.path().join("task-models.sqlite"))
            .await
            .unwrap();
        for id in ["owner", "other"] {
            let account = VirtualAccount {
                provider_id: "chatgpt".into(),
                id: id.into(),
                username: id.into(),
                password_hash: "fixture".into(),
                name: id.into(),
                email: format!("{id}@test.invalid"),
                plan_type: "pro".into(),
                plan_id: "pro".into(),
                subscription_expires_at: None,
                enabled: true,
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            storage.save_virtual_account(&account).await.unwrap();
        }
        let mut supplier = codex2api_storage::NewSupplierAccount::pending_identity(
            "installation",
            "originator",
            "ua",
            "os",
            "version",
            "arch",
            "",
            "{}",
        );
        supplier.id = Some("supplier".into());
        storage.create_account(supplier).await.unwrap();
        storage
            .save_virtual_resource(
                "owner",
                "task_execution",
                "task",
                Some("supplier"),
                &json!({"model":"gpt-6-astra"}),
            )
            .await
            .unwrap();
        let follow = json!({"follow_up":{"task_id":"task"}});
        assert_eq!(
            execution_model(&storage, "owner", "supplier", &follow)
                .await
                .unwrap(),
            "gpt-6-astra"
        );
        assert!(
            execution_model(&storage, "other", "supplier", &follow)
                .await
                .is_err()
        );
        assert!(
            execution_model(&storage, "owner", "rebound", &follow)
                .await
                .is_err()
        );
        assert!(
            execution_model(&storage, "owner", "supplier", &json!({"new_task":{}}))
                .await
                .is_err()
        );
        assert_eq!(
            execution_model(
                &storage,
                "owner",
                "supplier",
                &json!({"follow_up":{"task_id":"task"},"metadata":{"model_slug":"gpt-5.6-luna"}})
            )
            .await
            .unwrap(),
            "gpt-5.6-luna"
        );
    }
}

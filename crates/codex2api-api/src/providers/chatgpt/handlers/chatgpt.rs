use crate::{ApiState, Result};
use axum::{
    body::{Body, Bytes},
    extract::Path,
    extract::{Extension, OriginalUri, State},
    http::HeaderMap,
    response::Response,
};
use codex2api_storage::VirtualAccess;
use codex2api_upstream::ChatgptEndpoint;

pub async fn virtual_profile(
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> Result<Response> {
    let account = state
        .storage
        .virtual_account(&access.virtual_account_id)
        .await?
        .ok_or_else(crate::ApiError::invalid_token)?;
    let mut profile = crate::providers::chatgpt::identity::identity(&account);
    profile["id"] = format!("user-{}", account.id).into();
    let private = state
        .storage
        .virtual_config(&account.id, "profile")
        .await?
        .value;
    profile["picture"] = private["picture"].clone();
    profile["bio"] = private["bio"].clone();
    Ok(crate::providers::chatgpt::identity::json_response(profile))
}

pub async fn optimized_account_check(
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> Result<Response> {
    let account = state
        .storage
        .virtual_account(&access.virtual_account_id)
        .await?
        .ok_or_else(crate::ApiError::invalid_token)?;
    let mut value = crate::providers::chatgpt::identity::optimized_account_check(&account);
    let settings = state
        .storage
        .virtual_config(&account.id, "account_settings")
        .await?
        .value;
    value["permissions"] = settings["permissions"].clone();
    Ok(crate::providers::chatgpt::identity::json_response(value))
}

pub async fn virtual_settings(
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> Result<Response> {
    let mut value = state
        .storage
        .virtual_config(&access.virtual_account_id, "user_settings")
        .await?
        .value;
    let voice = state
        .storage
        .virtual_config(&access.virtual_account_id, "voice")
        .await?
        .value;
    if let Some(selected) = voice["selected"].as_str() {
        value["settings"]["voice_name"] = selected.into();
    } else {
        // Desktop's complete user-settings schema accepts an absent voice name,
        // but not null. A null here disables unrelated settings (including Ultra).
        value["settings"]
            .as_object_mut()
            .unwrap()
            .remove("voice_name");
    }
    Ok(crate::providers::chatgpt::identity::json_response(value))
}
pub async fn virtual_auto_top_up(
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> Result<Response> {
    Ok(crate::providers::chatgpt::identity::json_response(
        state
            .storage
            .virtual_config(&access.virtual_account_id, "auto_top_up")
            .await?
            .value,
    ))
}
pub async fn virtual_discount_offer(
    State(state): State<ApiState>,
    Extension(access): Extension<VirtualAccess>,
) -> Result<Response> {
    Ok(crate::providers::chatgpt::identity::json_response(
        state
            .storage
            .virtual_config(&access.virtual_account_id, "discount_offer")
            .await?
            .value,
    ))
}

pub async fn forward(
    State(state): State<ApiState>,
    Extension(endpoint): Extension<ChatgptEndpoint>,
    oauth: Extension<VirtualAccess>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let id = oauth.virtual_account_id.clone();
    let account = state
        .storage
        .virtual_account(&id)
        .await?
        .ok_or_else(crate::ApiError::invalid_token)?;
    let query: Vec<_> = url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()).collect();
    if query.iter().any(|(k, v)| k == "account_id" && v != &id) {
        return Err(crate::ApiError::bad_request(
            "SupplierAccount does not match virtual identity.",
        ));
    }
    if endpoint == ChatgptEndpoint::Privacy {
        let features: Vec<_> = query.iter().filter(|(k, _)| k == "feature").collect();
        let values: Vec<_> = query.iter().filter(|(k, _)| k == "value").collect();
        if features.len() != 1 || values.len() != 1 {
            return Err(crate::ApiError::bad_request(
                "Expected one feature and value.",
            ));
        }
        let feature = features[0].1.as_ref();
        let raw = values[0].1.as_ref();
        if feature == "voice_name" {
            if raw.is_empty()
                || raw.len() > 128
                || !raw
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            {
                return Err(crate::ApiError::bad_request("Invalid voice selection."));
            }
            let saved = state.storage.virtual_config(&id, "voice").await?;
            super::virtual_data::save_config(
                &state,
                &oauth.0,
                "voice",
                &serde_json::json!({"selected":raw}),
                saved.revision,
            )
            .await?;
        } else {
            if feature.is_empty() || feature.len() > 128 {
                return Err(crate::ApiError::bad_request("Invalid user preference."));
            }
            let saved = state.storage.virtual_config(&id, "user_settings").await?;
            let mut value = saved.value;
            let spec = codex2api_storage::client_fields("user_settings")
                .into_iter()
                .find(|spec| spec.path == ["settings", feature]);
            value["settings"][feature] = match spec.as_ref().map(|spec| spec.kind) {
                Some("select" | "date" | "text" | "string") => raw.into(),
                _ if matches!(raw, "true" | "false") => (raw == "true").into(),
                _ => {
                    return Err(crate::ApiError::bad_request(
                        "Invalid user preference value.",
                    ));
                }
            };
            codex2api_storage::validate_client_fields("user_settings", &value)
                .map_err(crate::ApiError::bad_request)?;
            super::virtual_data::save_config(
                &state,
                &oauth.0,
                "user_settings",
                &value,
                saved.revision,
            )
            .await?;
        }
        return Ok(crate::providers::chatgpt::identity::json_response(
            serde_json::json!({"success":true}),
        ));
    }
    if endpoint == ChatgptEndpoint::InstalledPlugins {
        let mut value = state
            .storage
            .virtual_config(&id, "installed_plugins")
            .await?
            .value;
        // Desktop's bundled app-server requires a pagination object even when
        // the virtual account has no installed plugins. Protocol metadata is
        // generated here, including for configurations saved before this fix.
        let limit_values: Vec<_> = query.iter().filter(|(key, _)| key == "limit").collect();
        let token_values: Vec<_> = query.iter().filter(|(key, _)| key == "pageToken").collect();
        if limit_values.len() > 1 || token_values.len() > 1 {
            return Err(crate::ApiError::bad_request(
                "Duplicate plugin pagination parameter.",
            ));
        }
        let limit = limit_values
            .first()
            .map(|(_, v)| v.parse::<usize>())
            .transpose()
            .map_err(|_| crate::ApiError::bad_request("Invalid plugin limit."))?
            .unwrap_or(200);
        if !(1..=200).contains(&limit) {
            return Err(crate::ApiError::bad_request(
                "Plugin limit must be between 1 and 200.",
            ));
        }
        let offset = token_values
            .first()
            .map(|(_, v)| v.parse::<usize>())
            .transpose()
            .map_err(|_| crate::ApiError::bad_request("Invalid plugin page token."))?
            .unwrap_or(0);
        let plugins = value["plugins"]
            .as_array_mut()
            .ok_or_else(|| crate::ApiError::internal("Invalid virtual plugin records."))?;
        let total = plugins.len();
        *plugins = plugins.iter().skip(offset).take(limit).cloned().collect();
        let next = offset.saturating_add(limit);
        value["pagination"] = serde_json::json!({"limit":limit,"next_page_token":if next<total{Some(next.to_string())}else{None}});
        value.as_object_mut().unwrap().remove("nextPageToken");
        return Ok(crate::providers::chatgpt::identity::json_response(value));
    }
    if endpoint == ChatgptEndpoint::StatsigBootstrap {
        let request = if body.is_empty() {
            serde_json::json!({})
        } else {
            codex2api_upstream::decode_body(&body, &headers)?
        };
        if !request.is_object() {
            return Err(crate::ApiError::bad_request(
                "Expected a desktop bootstrap object.",
            ));
        }
        let beta = request
            .get("desktop_app_beta_enabled")
            .map(|v| {
                v.as_bool()
                    .ok_or_else(|| crate::ApiError::bad_request("Invalid desktop beta state."))
            })
            .transpose()?
            .unwrap_or(false);
        let mut custom_ids = serde_json::json!({"account_id":account.id});
        if let Some(stable_id) = request.get("stable_id").filter(|v| !v.is_null()) {
            let stable_id = stable_id
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
                .ok_or_else(|| crate::ApiError::bad_request("Invalid desktop stable_id."))?;
            custom_ids["stableID"] = stable_id.into();
        }
        let access = &oauth.0;
        let payload = super::desktop_support::bootstrap(
            &state,
            access,
            beta,
            custom_ids["stableID"].as_str().map(str::to_owned),
        )
        .await?;
        return Ok(crate::providers::chatgpt::identity::json_response(
            serde_json::json!({"statsigPayload":payload.to_string()}),
        ));
    }
    if matches!(
        endpoint,
        ChatgptEndpoint::AnalyticsEvents | ChatgptEndpoint::Traces
    ) {
        if endpoint == ChatgptEndpoint::AnalyticsEvents {
            let value = codex2api_upstream::decode_body(&body, &headers)?;
            let events = value["events"]
                .as_array()
                .filter(|v| v.len() <= 1000)
                .ok_or_else(|| {
                    crate::ApiError::bad_request("Expected at most 1000 analytics events.")
                })?;
            state.storage.record_virtual_analytics(&id, events).await?;
        }
        // Trace bodies are deliberately not retained; require_oauth records this
        // request's account/device/status. Nothing is sent under supplier identity.
        return Ok(crate::providers::chatgpt::identity::json_response(
            serde_json::json!({}),
        ));
    }
    match endpoint {
        ChatgptEndpoint::AccountsCheck => {
            let mut value = crate::providers::chatgpt::identity::account_check(&account);
            let settings = state
                .storage
                .virtual_config(&account.id, "account_settings")
                .await?
                .value;
            value["accounts"][&account.id]["permissions"] = settings["permissions"].clone();
            return Ok(crate::providers::chatgpt::identity::json_response(value));
        }
        ChatgptEndpoint::Subscriptions => {
            return Ok(crate::providers::chatgpt::identity::json_response(
                crate::providers::chatgpt::identity::subscriptions(&account),
            ));
        }
        ChatgptEndpoint::Privacy
        | ChatgptEndpoint::FeaturedPlugins
        | ChatgptEndpoint::Plugins
        | ChatgptEndpoint::InstalledPlugins
        | ChatgptEndpoint::SuggestedPlugins
        | ChatgptEndpoint::AnalyticsEvents
        | ChatgptEndpoint::StatsigBootstrap
        | ChatgptEndpoint::Traces
        | ChatgptEndpoint::ConnectorDirectory
        | ChatgptEndpoint::SiteStatus
        | ChatgptEndpoint::Voices => {}
    }
    if matches!(
        endpoint,
        ChatgptEndpoint::FeaturedPlugins
            | ChatgptEndpoint::Plugins
            | ChatgptEndpoint::SuggestedPlugins
            | ChatgptEndpoint::ConnectorDirectory
    ) {
        return Err(catalog_unavailable());
    }
    let (_, ctx) =
        crate::providers::chatgpt::access::resolve_supplier(&state, &headers, oauth).await?;
    let account_id = ctx.account.chatgpt_account_id.as_deref().unwrap_or("");
    let translated_query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(query.iter().map(|(key, value)| {
            (
                key.as_ref(),
                if key == "account_id" {
                    account_id
                } else {
                    value.as_ref()
                },
            )
        }))
        .finish();
    endpoint.url(account_id, Some(&translated_query))?;
    let upstream = state.upstream.get(&ctx.account.id).await?;
    let response = upstream
        .forward_chatgpt(endpoint, account_id, Some(&translated_query), body, headers)
        .await?;
    if !response.status().is_success() {
        return Err(crate::ApiError::openai(
            axum::http::StatusCode::BAD_GATEWAY,
            "api_error",
            "The provider service request failed.",
            Some("provider_request_failed"),
        ));
    }
    if endpoint == ChatgptEndpoint::SiteStatus {
        let value: serde_json::Value = response.json().await.map_err(|_| {
            crate::ApiError::openai(
                axum::http::StatusCode::BAD_GATEWAY,
                "api_error",
                "Invalid desktop service response.",
                Some("invalid_upstream_response"),
            )
        })?;
        let features = site_features(&value)?;
        let site = query
            .iter()
            .find(|(k, _)| k == "site_url")
            .and_then(|(_, v)| url::Url::parse(v).ok())
            .and_then(|u| u.host_str().map(str::to_owned))
            .ok_or_else(|| crate::ApiError::bad_request("Invalid site URL."))?;
        state.storage.save_virtual_resource(&id,"site_status",&site,None,&serde_json::json!({"host":site,"feature_status":features,"checked_at":chrono::Utc::now().to_rfc3339()})).await?;
        return Ok(crate::providers::chatgpt::identity::json_response(
            serde_json::json!({"feature_status":features}),
        ));
    }
    if endpoint == ChatgptEndpoint::Voices && response.status().is_success() {
        let mut value: serde_json::Value = response
            .json()
            .await
            .map_err(|_| crate::ApiError::internal("Invalid voices response."))?;
        if !value.get("voices").is_some_and(serde_json::Value::is_array) {
            return Err(crate::ApiError::internal("Missing voice catalog."));
        }
        value["selected"] = state
            .storage
            .virtual_client_state(&id, "voice")
            .await?
            .map(|s| s.value["selected"].clone())
            .unwrap_or(serde_json::Value::Null);
        return Ok(crate::providers::chatgpt::identity::json_response(value));
    }

    Ok(crate::response::forward_response(
        response.status(),
        response.headers().clone(),
        Body::from_stream(response.bytes_stream()),
    ))
}

pub async fn plugin_detail(
    State(state): State<ApiState>,
    Path(plugin_id): Path<String>,
    oauth: Extension<VirtualAccess>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    if plugin_id.is_empty()
        || plugin_id.len() > 256
        || !plugin_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(crate::ApiError::bad_request("Invalid plugin identifier."));
    }
    let _ = (state, oauth, uri, headers, body);
    Err(catalog_unavailable())
}

pub async fn mcp(
    State(state): State<ApiState>,
    oauth: Extension<VirtualAccess>,
    OriginalUri(_uri): OriginalUri,
    headers: HeaderMap,
    method: http::Method,
    body: Bytes,
) -> Result<Response> {
    let access = oauth.0;
    let value = if method == http::Method::POST {
        codex2api_upstream::decode_body(&body, &headers)?
    } else {
        serde_json::json!({})
    };
    let method = value["method"].as_str().unwrap_or("transport");
    let tool = value["params"]["name"].as_str().unwrap_or("");
    if method.len() > 128 || tool.len() > 256 {
        return Err(crate::ApiError::bad_request("Invalid MCP request."));
    }
    // A supplier login is not a virtual-account resource grant. Until an
    // account-owned connector executor exists, no private RPC may reach it.
    state.storage.save_virtual_resource(&access.virtual_account_id,"mcp_operation",&uuid::Uuid::new_v4().to_string(),None,&serde_json::json!({"method":method,"tool":tool,"status":"authorization_unavailable","created_at_ms":chrono::Utc::now().timestamp_millis()})).await?;
    let mut response = crate::providers::chatgpt::identity::json_response(
        serde_json::json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32003,"message":"Virtual-account connector authorization and execution are unavailable.","data":{"code":"connector_authorization_unavailable"}}}),
    );
    *response.status_mut() = http::StatusCode::NOT_IMPLEMENTED;
    Ok(response)
}

pub async fn apps_batch(
    State(state): State<ApiState>,
    oauth: Extension<VirtualAccess>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    let _ = (state, oauth, uri, headers, body);
    Err(catalog_unavailable())
}

pub async fn mcp_metadata(State(state): State<ApiState>, headers: HeaderMap) -> Result<Response> {
    let origin = if let Some(origin) = state.public_base_url {
        origin
    } else {
        let host = headers
            .get(http::header::HOST)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                crate::ApiError::bad_request("Host is required for resource discovery.")
            })?;
        let authority = host
            .parse::<http::uri::Authority>()
            .map_err(|_| crate::ApiError::bad_request("Invalid Host."))?;
        if authority.as_str().contains('@') {
            return Err(crate::ApiError::bad_request("Invalid Host."));
        }
        let url = url::Url::parse(&format!("http://{authority}"))
            .map_err(|_| crate::ApiError::bad_request("Invalid Host."))?;
        url.origin().ascii_serialization()
    };
    // Existing virtual-account tokens authenticate this resource; don't advertise OpenAI's issuer.
    Ok(crate::providers::chatgpt::identity::json_response(
        serde_json::json!({
            "resource":format!("{origin}{}/backend-api/ps/mcp",super::oauth::PREFIX),
            "bearer_methods_supported":["header"],
            "scopes_supported":["api.connectors.read","api.connectors.invoke"]
        }),
    ))
}

fn catalog_unavailable() -> crate::ApiError {
    crate::ApiError::openai(
        axum::http::StatusCode::NOT_IMPLEMENTED,
        "api_error",
        "Virtual-account plugin and connector catalog authorization is unavailable.",
        Some("catalog_authorization_unavailable"),
    )
}

fn site_features(value: &serde_json::Value) -> Result<&serde_json::Map<String, serde_json::Value>> {
    value["feature_status"]
        .as_object()
        .filter(|v| v.values().all(serde_json::Value::is_boolean))
        .ok_or_else(invalid_service_response)
}
fn invalid_service_response() -> crate::ApiError {
    crate::ApiError::openai(
        axum::http::StatusCode::BAD_GATEWAY,
        "api_error",
        "Invalid desktop service response.",
        Some("invalid_upstream_response"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn desktop_site_policy_follows_client_contract() {
        let denied = json!({"feature_status":{"agent":true,"page_content":false}});
        assert_eq!(site_features(&denied).unwrap()["agent"], true);
        assert_eq!(site_features(&denied).unwrap()["page_content"], false);
        assert!(site_features(&json!({"feature_status":{"agent":"false"}})).is_err());
        assert!(site_features(&json!({})).is_err());
    }
}

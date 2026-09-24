//! Local host enrollment. Tokens belong to a virtual account's authenticated device.
use axum::{
    Json,
    extract::{
        Extension, State, WebSocketUpgrade,
        ws::{CloseFrame, Message},
    },
    http::HeaderMap,
    response::Response,
};
use codex2api_storage::{RemoteServer, RemoteServerRegistration, VirtualAccess};
use serde::Deserialize;
use serde_json::json;

fn credentials() -> (String, i64) {
    (
        format!(
            "rc_{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ),
        chrono::Utc::now().timestamp() + 3600,
    )
}
fn response(server: RemoteServer, token: String) -> Response {
    crate::providers::chatgpt::identity::json_response(
        json!({"server_id":server.id,"environment_id":server.environment_id,"remote_control_token":token,"expires_at":chrono::DateTime::from_timestamp(server.expires_at,0).unwrap().to_rfc3339()}),
    )
}
fn check_installation(headers: &HeaderMap, installation: &str) -> crate::Result<()> {
    if headers
        .get("x-codex-installation-id")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|id| id != installation)
    {
        return Err(crate::ApiError::bad_request("Installation mismatch."));
    }
    Ok(())
}
pub(crate) async fn enroll(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    headers: HeaderMap,
    Json(input): Json<RemoteServerRegistration>,
) -> crate::Result<Response> {
    for text in [
        &input.name,
        &input.os,
        &input.arch,
        &input.app_server_version,
        &input.installation_id,
    ] {
        if text.trim().is_empty() || text.len() > 256 || text.chars().any(char::is_control) {
            return Err(crate::ApiError::bad_request(
                "Invalid remote host registration.",
            ));
        }
    }
    check_installation(&headers, &input.installation_id)?;
    let (token, expires) = credentials();
    let server = state
        .storage
        .enroll_remote_server(&access, &input, &token, expires)
        .await?;
    Ok(response(server, token))
}

#[derive(Deserialize)]
pub(crate) struct Refresh {
    server_id: String,
    installation_id: String,
}
pub(crate) async fn refresh(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    headers: HeaderMap,
    Json(input): Json<Refresh>,
) -> crate::Result<Response> {
    check_installation(&headers, &input.installation_id)?;
    let (token, expires) = credentials();
    let server = state
        .storage
        .refresh_remote_server(
            &access,
            &input.server_id,
            &input.installation_id,
            &token,
            expires,
        )
        .await?
        .ok_or_else(super::virtual_data::not_found)?;
    Ok(response(server, token))
}

pub(crate) async fn socket(
    State(state): State<crate::ApiState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> crate::Result<Response> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(crate::ApiError::invalid_token)?
        .to_owned();
    let server = state
        .storage
        .remote_server_for_token(&token)
        .await?
        .ok_or_else(crate::ApiError::invalid_token)?;
    if headers
        .get("x-codex-server-id")
        .and_then(|v| v.to_str().ok())
        != Some(server.id.as_str())
    {
        return Err(crate::ApiError::invalid_token());
    }
    check_installation(&headers, &server.installation_id)?;
    Ok(ws.on_upgrade(move |mut socket| async move {
        let connection = uuid::Uuid::new_v4().to_string();
        if state.storage.connect_remote_server(&server.id,&connection).await.is_err() {return;}
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if !matches!(state.storage.remote_server_for_token(&token).await,Ok(Some(_))) || !matches!(state.storage.touch_remote_server(&server.id,&connection).await,Ok(true)) {break;}
                    if socket.send(Message::Ping(Vec::new().into())).await.is_err() {break;}
                }
                message = socket.recv() => match message {
                    Some(Ok(Message::Ping(data))) => {if socket.send(Message::Pong(data)).await.is_err() {break;}},
                    Some(Ok(Message::Pong(_))) => {},
                    // No controller is authorized by host enrollment. Do not acknowledge
                    // application messages as delivered without an actual paired controller.
                    Some(Ok(Message::Text(_) | Message::Binary(_))) => {
                        let _ = socket.send(Message::Close(Some(CloseFrame {code:1008,reason:"No authorized remote controller stream.".into()}))).await;
                        break;
                    }
                    _ => break,
                }
            }
        }
        let _ = socket.send(Message::Close(None)).await;
        let _ = state.storage.disconnect_remote_server(&server.id,&connection).await;
    }))
}

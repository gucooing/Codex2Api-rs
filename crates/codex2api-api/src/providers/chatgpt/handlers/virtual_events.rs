//! Virtual-account-scoped desktop pubsub backed by durable virtual events.
use axum::{
    extract::{
        Extension, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::HeaderMap,
    response::Response,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use codex2api_storage::VirtualAccess;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
struct Ticket {
    scope: String,
    hash: String,
    expires: i64,
}
#[derive(Deserialize)]
pub(crate) struct TicketQuery {
    ticket: String,
}
const SOCKET_PATH: &str = "/api/oauth/chatgpt/backend-api/celsius/ws/user/socket";

pub(crate) async fn connection(
    State(state): State<crate::ApiState>,
    Extension(access): Extension<VirtualAccess>,
    headers: HeaderMap,
) -> crate::Result<Response> {
    let origin = if let Some(origin) = &state.public_base_url {
        origin.clone()
    } else {
        let host = headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| crate::ApiError::bad_request("Missing Host."))?;
        let authority = host
            .parse::<http::uri::Authority>()
            .map_err(|_| crate::ApiError::bad_request("Invalid Host."))?;
        if authority.as_str().contains('@') {
            return Err(crate::ApiError::bad_request("Invalid Host."));
        }
        format!("http://{authority}")
    };
    let mut url = url::Url::parse(&origin)
        .map_err(|_| crate::ApiError::bad_request("Invalid public origin."))?;
    let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(scheme)
        .map_err(|_| crate::ApiError::internal("Invalid WebSocket scheme."))?;
    url.set_path(SOCKET_PATH);
    let ticket = Ticket {
        scope: SOCKET_PATH.into(),
        hash: access.token_hash,
        expires: chrono::Utc::now().timestamp() + 1200,
    };
    let payload = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&ticket)
            .map_err(|_| crate::ApiError::internal("Ticket encoding failed."))?,
    );
    let secret = state.storage.oauth_signing_key().await?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC key");
    mac.update(payload.as_bytes());
    url.query_pairs_mut().append_pair(
        "ticket",
        &format!(
            "{payload}.{}",
            URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
        ),
    );
    Ok(crate::providers::chatgpt::identity::json_response(
        json!({"websocket_url":url.as_str()}),
    ))
}

pub(crate) async fn socket(
    State(state): State<crate::ApiState>,
    Query(query): Query<TicketQuery>,
    ws: WebSocketUpgrade,
) -> crate::Result<Response> {
    let (payload, signature) = query
        .ticket
        .split_once('.')
        .filter(|_| query.ticket.len() < 2048)
        .ok_or_else(crate::ApiError::invalid_token)?;
    let secret = state.storage.oauth_signing_key().await?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC key");
    mac.update(payload.as_bytes());
    mac.verify_slice(
        &URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| crate::ApiError::invalid_token())?,
    )
    .map_err(|_| crate::ApiError::invalid_token())?;
    let ticket: Ticket = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| crate::ApiError::invalid_token())?,
    )
    .map_err(|_| crate::ApiError::invalid_token())?;
    if ticket.scope != SOCKET_PATH || ticket.expires <= chrono::Utc::now().timestamp() {
        return Err(crate::ApiError::invalid_token());
    }
    let access = state
        .storage
        .virtual_access(&ticket.hash)
        .await?
        .ok_or_else(crate::ApiError::invalid_token)?;
    Ok(ws
        .max_message_size(64 * 1024)
        .on_upgrade(move |socket| run(state, socket, access, ticket.expires)))
}

async fn run(state: crate::ApiState, mut socket: WebSocket, access: VirtualAccess, expires: i64) {
    let mut topics: HashMap<String, i64> = HashMap::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        tokio::select! {
            _=interval.tick()=>{
                if chrono::Utc::now().timestamp()>=expires || !state.storage.virtual_access(&access.token_hash).await.is_ok_and(|v|v.is_some()) {let _=socket.send(Message::Close(None)).await;break;}
                for (topic,offset) in &mut topics {
                    let Ok(events)=state.storage.virtual_events(&access.virtual_account_id,*offset).await else {return};
                    let mut frames=Vec::new();
                    for e in events {
                        let id=e["id"].as_i64().unwrap_or(*offset);*offset=id;
                        if e["topic"].as_str()==Some(topic){frames.push(json!({"type":"message","topic_id":topic,"offset":id.to_string(),"payload":e["payload"]}));}
                    }
                    if !frames.is_empty() && socket.send(Message::Text(json!(frames).to_string().into())).await.is_err(){return;}
                }
            }
            message=socket.recv()=>{
                let Some(Ok(message))=message else {break};
                let Message::Text(text)=message else {if matches!(message,Message::Close(_)){break;}continue};
                let Ok(commands)=serde_json::from_str::<Vec<Value>>(&text) else {break};
                if commands.len()>100 {break;}
                let mut replies=Vec::new();
                for command in commands {
                    let Some(id)=command["id"].as_i64() else {continue};
                    let c=&command["command"];
                    let reply=match c["type"].as_str() {
                        Some("connect"|"presence")=>json!({"type":c["type"]}),
                        Some("subscribe")=>{
                            let Some(topic)=c["topic_id"].as_str().filter(|s|matches!(*s,"app_notifications"|"conversations"|"alder-conversations"|"settings")) else {replies.push(json!({"id":id,"reply":{"type":"error","code":"unknown_topic"}}));continue};
                            let offset=c["offset"].as_str().and_then(|s|s.parse::<i64>().ok()).filter(|n|*n>=0).unwrap_or(0);
                            topics.insert(topic.into(),offset);
                            json!({"type":"subscribe","topic_id":topic,"recovered":true,"last_offset":offset.to_string(),"catchups":[]})
                        }
                        Some("unsubscribe")=>{if let Some(topic)=c["topic_id"].as_str(){topics.remove(topic);}json!({"type":"unsubscribe"})}
                        _=>json!({"type":"error","code":"unknown_command"}),
                    };
                    replies.push(json!({"id":id,"reply":reply}));
                }
                if socket.send(Message::Text(json!(replies).to_string().into())).await.is_err(){break;}
            }
        }
    }
}

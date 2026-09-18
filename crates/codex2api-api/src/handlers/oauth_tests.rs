use super::*;
use axum::{
    Extension, Router,
    body::{Body, to_bytes},
    extract::WebSocketUpgrade,
    http::Request,
    routing::post,
};
use codex2api_accounts::{AccountStore, AuthDotJson, TokenData};
use codex2api_storage::{AccountStatus, AccountUpdate, OAuthAccess, Storage};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
use tower::ServiceExt;

#[tokio::test]
async fn issued_token_resolves_bound_account_and_ws_stops_after_revocation() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path().join("oauth-ws.sqlite"))
        .await
        .unwrap();
    let accounts = AccountStore::open(storage.clone());
    let account = accounts.create_pending().await.unwrap().account;
    storage
        .update_account(
            &account.id,
            AccountUpdate {
                status: Some(AccountStatus::Active),
                chatgpt_account_id: Some("bound-account".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    accounts
        .save_auth_for_account(
            &account.id,
            &AuthDotJson::chatgpt(
                TokenData {
                    id_token: "official-id".into(),
                    access_token: "official-access".into(),
                    refresh_token: "official-refresh".into(),
                    account_id: Some("bound-account".into()),
                },
                None,
            ),
        )
        .await
        .unwrap();
    let credential = storage
        .create_oauth_credential(&account.id, "ws-client")
        .await
        .unwrap();
    let refresh = storage
        .oauth_refresh_token(&credential.id)
        .await
        .unwrap()
        .unwrap();
    let auth = codex2api_auth::AuthService::new(accounts.clone()).unwrap();
    let state = ApiState::new(
        storage.clone(),
        accounts,
        codex2api_upstream::UpstreamPool::new(auth),
    );
    let response = crate::router(state.clone())
        .oneshot(
            Request::post(format!("{PREFIX}/oauth/token"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"grant_type":"refresh_token","refresh_token":refresh}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let tokens: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let access = tokens["access_token"].as_str().unwrap();
    // Check the emitted JWT signature, independently of hash-based access lookup.
    let (input, signature) = access.rsplit_once('.').unwrap();
    let secret = storage.oauth_signing_key().await.unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(input.as_bytes());
    mac.verify_slice(&URL_SAFE_NO_PAD.decode(signature).unwrap())
        .unwrap();

    let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream_listener.local_addr().unwrap();
    let upstream_server = tokio::spawn(async move {
        let (stream, _) = upstream_listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let first = socket.next().await.unwrap().unwrap();
        socket.send(first).await.unwrap();
        let next = socket.next().await;
        assert!(!matches!(
            next,
            Some(Ok(Message::Text(_))) | Some(Ok(Message::Binary(_)))
        ));
    });
    let path = format!("{PREFIX}/backend-api/codex/responses");
    // Local fixture exercises the production OAuth middleware, account resolver and WS bridge.
    let app = Router::new().route(&path, post(
        |State(state): State<ApiState>, oauth: Option<Extension<OAuthAccess>>, headers: HeaderMap| async move {
            let (credential,ctx) = crate::auth::authenticate_request(&state,&headers,oauth).await?;
            let mut record = crate::usage::UsageContext::new(state.storage.clone(), &ctx.account, &credential.id, &credential.name, "/v1/responses", "http")
                .start(Default::default(),std::time::Instant::now(),chrono::Utc::now().timestamp_millis()).await?;
            record.finish("completed");
            Ok::<_,ApiError>(Json(json!({"account_id":ctx.account.id,"installation_id":ctx.identity.installation_id})))
        }
    ).get(move |State(state): State<ApiState>, oauth: Option<Extension<OAuthAccess>>, headers: HeaderMap, upgrade: WebSocketUpgrade| async move {
        let (credential,ctx) = crate::auth::authenticate_request(&state,&headers,oauth).await?;
        let (upstream,_) = tokio_tungstenite::client_async_tls_with_config(
            format!("ws://{upstream_addr}/responses"),
            tokio_tungstenite::MaybeTlsStream::Plain(tokio::net::TcpStream::connect(upstream_addr).await.unwrap()), None,None,
        ).await.unwrap();
        Ok::<_,ApiError>(upgrade.on_upgrade(move |socket| crate::handlers::websocket::bridge(socket,upstream,ctx.identity.installation_id,true,Some((state.storage,credential.access)))))
    })).route_layer(axum::middleware::from_fn_with_state(state.clone(),require_oauth)).with_state(state);
    let response = app
        .clone()
        .oneshot(
            Request::post(&path)
                .header("authorization", format!("Bearer {access}"))
                .header("chatgpt-account-id", "bound-account")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let resolved: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(resolved["account_id"], account.id);
    assert_eq!(resolved["installation_id"], account.installation_id);
    let usage = storage.query_usage(&Default::default()).await.unwrap();
    assert_eq!(usage.records[0].api_key_id, credential.id);
    assert_eq!(usage.records[0].api_key_name, "ws-client");
    let devices = storage
        .oauth_devices_for_account(&account.id)
        .await
        .unwrap();
    assert_eq!(devices.len(), 1);
    assert!(devices[0].last_used_at.is_some());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let proxy = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut request = format!("ws://{addr}{path}").into_client_request().unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {access}").parse().unwrap());
        let (mut client, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        client
            .send(Message::Text(
                r#"{"type":"session.update","session":{}}"#.into(),
            ))
            .await
            .unwrap();
        assert!(matches!(client.next().await, Some(Ok(Message::Text(_)))));
        storage.revoke_oauth_token(&refresh).await.unwrap();
        client
            .send(Message::Text(
                r#"{"type":"session.update","session":{"after":"revoke"}}"#.into(),
            ))
            .await
            .unwrap();
        assert!(!matches!(client.next().await, Some(Ok(Message::Text(_)))));
        upstream_server.await.unwrap();
    })
    .await;
    proxy.abort();
    result.unwrap();
    storage.close().await;
}

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use codex2api_accounts::{
    AccountIdentity, AuthDotJson, HostRuntime, SupplierAccountStore, new_installation_id,
};
use codex2api_auth::persist::{bind_completed_login, chatgpt_auth, pending_from_account};
use codex2api_auth::{AuthError, AuthService, OAuthConfig, parse_chatgpt_jwt_claims};
use codex2api_storage::Storage;
use serde_json::json;

fn identity() -> AccountIdentity {
    AccountIdentity::new(
        uuid::Uuid::new_v4().to_string(),
        new_installation_id(),
        HostRuntime::generate(),
    )
}

fn credentials(user: Option<&str>, workspace: &str, plan: &str, marker: &str) -> AuthDotJson {
    let payload = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&json!({
            "email": format!("{}@example.test", user.unwrap_or("unknown")),
            "https://api.openai.com/auth": {
                "chatgpt_user_id": user, "chatgpt_account_id": workspace, "chatgpt_plan_type": plan
            }
        }))
        .unwrap(),
    );
    chatgpt_auth(
        format!("e30.{payload}.c2ln"),
        format!("access-{marker}"),
        format!("refresh-{marker}"),
        Some(workspace.into()),
    )
}

#[tokio::test]
async fn team_members_and_personal_subscriptions_keep_separate_credentials_and_fingerprints() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owners.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let accounts = SupplierAccountStore::open(storage.clone());
    let mut originals = Vec::new();
    for (user, workspace, plan) in [
        ("alice", "shared-team", "business"),
        ("bob", "shared-team", "business"),
        ("alice", "personal", "pro"),
    ] {
        let draft = identity();
        let auth = credentials(Some(user), workspace, plan, &format!("{user}-{workspace}"));
        let claims = parse_chatgpt_jwt_claims(&auth.tokens.as_ref().unwrap().id_token).unwrap();
        let saved = accounts
            .save_authorized_identity(&draft, claims.oauth_identity().unwrap(), &auth, None)
            .await
            .unwrap();
        assert!(!saved.reused_existing);
        assert_eq!(saved.account.id, draft.account_id);
        assert_eq!(saved.account.installation_id, draft.installation_id);
        assert_eq!(saved.account.plan_type.as_deref(), Some(plan));
        originals.push((saved.account, auth));
    }
    assert_eq!(storage.list_accounts().await.unwrap().len(), 3);
    for (original, _) in &originals {
        // Exercise the legacy pending-row path as well as the atomic draft path.
        let auth = credentials(
            original.chatgpt_user_id.as_deref(),
            original.chatgpt_account_id.as_deref().unwrap(),
            original.plan_type.as_deref().unwrap(),
            &original.id,
        );
        let claims = parse_chatgpt_jwt_claims(&auth.tokens.as_ref().unwrap().id_token).unwrap();
        let pending = accounts.create_pending().await.unwrap();
        let saved = bind_completed_login(&accounts, &pending, &claims, &auth)
            .await
            .unwrap();
        assert!(saved.reused_existing);
        assert_eq!(saved.account.id, original.id);
        assert_eq!(saved.account.installation_id, original.installation_id);
        assert_eq!(
            saved.account.http_fingerprint_json,
            original.http_fingerprint_json
        );
        assert!(
            storage
                .get_account(&pending.account.id)
                .await
                .unwrap()
                .is_none()
        );
        let saved = accounts
            .save_authorized_identity(&identity(), claims.oauth_identity().unwrap(), &auth, None)
            .await
            .unwrap();
        assert_eq!(saved.account.id, original.id);
        assert_eq!(saved.account.installation_id, original.installation_id);
    }
    let service = AuthService::new(accounts.clone()).unwrap();
    let a = service.account_http(&originals[0].0.id).await.unwrap();
    let b = service.account_http(&originals[1].0.id).await.unwrap();
    let personal = service.account_http(&originals[2].0.id).await.unwrap();
    assert!(!std::sync::Arc::ptr_eq(&a, &b));
    assert!(!std::sync::Arc::ptr_eq(&a, &personal));
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    for (original, _) in &originals {
        let saved = reopened.require_account(&original.id).await.unwrap();
        assert_eq!(saved.installation_id, original.installation_id);
        assert_eq!(saved.chatgpt_user_id, original.chatgpt_user_id);
        assert_eq!(saved.chatgpt_account_id, original.chatgpt_account_id);
        assert_eq!(
            reopened
                .load_supplier_tokens(&saved.id)
                .await
                .unwrap()
                .unwrap()
                .access_token,
            Some(format!("access-{}", saved.id))
        );
    }
    reopened.close().await;
}

#[tokio::test]
async fn reauthorization_rejects_a_different_member_or_workspace_before_saving_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("relogin.sqlite"))
        .await
        .unwrap();
    let accounts = SupplierAccountStore::open(storage.clone());
    let initial = credentials(Some("alice"), "team", "business", "initial");
    let pending = accounts.create_pending().await.unwrap();
    let claims = parse_chatgpt_jwt_claims(&initial.tokens.as_ref().unwrap().id_token).unwrap();
    let saved = bind_completed_login(&accounts, &pending, &claims, &initial)
        .await
        .unwrap();
    let pending = pending_from_account(&accounts, &saved.account.id)
        .await
        .unwrap();
    for (user, workspace) in [
        (Some("bob"), "team"),
        (Some("alice"), "personal"),
        (None, "team"),
    ] {
        let auth = credentials(user, workspace, "pro", "wrong");
        let claims = parse_chatgpt_jwt_claims(&auth.tokens.as_ref().unwrap().id_token).unwrap();
        assert!(matches!(
            bind_completed_login(&accounts, &pending, &claims, &auth).await,
            Err(AuthError::AccountMismatch)
        ));
        assert_eq!(
            storage
                .load_supplier_tokens(&saved.account.id)
                .await
                .unwrap()
                .unwrap()
                .access_token
                .as_deref(),
            Some("access-initial")
        );
    }
    let unknown = credentials(None, "team", "business", "missing-user");
    assert!(
        parse_chatgpt_jwt_claims(&unknown.tokens.as_ref().unwrap().id_token)
            .unwrap()
            .oauth_identity()
            .is_err()
    );
    assert_eq!(storage.list_accounts().await.unwrap().len(), 1);
    storage.close().await;
}

#[tokio::test]
async fn refresh_rejects_another_team_member_without_overwriting_credentials() {
    use axum::{Json, Router, routing::post};
    use std::sync::{Arc, Mutex};
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("refresh.sqlite"))
        .await
        .unwrap();
    let accounts = SupplierAccountStore::open(storage.clone());
    let initial = credentials(Some("alice"), "team", "business", "initial");
    let claims = parse_chatgpt_jwt_claims(&initial.tokens.as_ref().unwrap().id_token).unwrap();
    let saved = accounts
        .save_authorized_identity(
            &identity(),
            claims.oauth_identity().unwrap(),
            &initial,
            None,
        )
        .await
        .unwrap();
    let response = Arc::new(Mutex::new(json!({})));
    let capture = response.clone();
    let app = Router::new().route(
        "/oauth/token",
        post(move || {
            let value = capture.lock().unwrap().clone();
            async move { Json(value) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = OAuthConfig {
        token_url: format!("http://{}/oauth/token", listener.local_addr().unwrap()),
        ..Default::default()
    };
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let service = AuthService::with_config(accounts, config).unwrap();
    for (user, workspace) in [("bob", "team"), ("alice", "personal"), ("alice", "team")] {
        let auth = credentials(Some(user), workspace, "business", "refreshed");
        let tokens = auth.tokens.unwrap();
        *response.lock().unwrap() = json!({"id_token":tokens.id_token,"access_token":tokens.access_token,"refresh_token":tokens.refresh_token});
        let result = service.refresh(&saved.account.id, true).await;
        if user == "alice" && workspace == "team" {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(AuthError::AccountMismatch)));
            assert_eq!(
                storage
                    .load_supplier_tokens(&saved.account.id)
                    .await
                    .unwrap()
                    .unwrap()
                    .access_token
                    .as_deref(),
                Some("access-initial")
            );
        }
    }
    assert_eq!(
        storage
            .load_supplier_tokens(&saved.account.id)
            .await
            .unwrap()
            .unwrap()
            .access_token
            .as_deref(),
        Some("access-refreshed")
    );
    server.abort();
    storage.close().await;
}

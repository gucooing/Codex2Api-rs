use super::*;
use axum::{
    Form, Json, Router,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use ring::signature::KeyPair;

#[test]
fn official_subscription_display_takes_precedence_over_internal_tier() {
    assert_eq!(
        subscription_display(
            &json!({"subscriptionTier":"SuperGrokHeavy","subscriptionTierDisplay":"SuperGrok Heavy"})
        ),
        Some("SuperGrok Heavy")
    );
    assert_eq!(
        subscription_display(&json!({"subscriptionTier":null,"subscriptionTierDisplay":"Free"})),
        Some("Free")
    );
    assert_eq!(
        subscription_display(&json!({"subscriptionTierDisplay":"New official name"})),
        Some("New official name")
    );
}

async fn fixture() -> (
    tempfile::TempDir,
    GrokAuthService,
    tokio::task::JoinHandle<()>,
) {
    let dir = tempfile::tempdir().unwrap();
    let storage = codex2api_storage::Storage::open(dir.path().join("grok.sqlite"))
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let rng = ring::rand::SystemRandom::new();
    let der = ring::signature::EcdsaKeyPair::generate_pkcs8(
        &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
        &rng,
    )
    .unwrap();
    let key = Arc::new(
        ring::signature::EcdsaKeyPair::from_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            der.as_ref(),
            &rng,
        )
        .unwrap(),
    );
    let point = key.public_key().as_ref();
    let jwks = json!({"keys":[{"kty":"EC","crv":"P-256","kid":"fixture","x":URL_SAFE_NO_PAD.encode(&point[1..33]),"y":URL_SAFE_NO_PAD.encode(&point[33..65])}]});
    let token_issuer = issuer.clone();
    let app=Router::new().route("/.well-known/jwks.json",get(move || {let value=jwks.clone();async move {Json(value)}}))
        .route("/v1/settings",get(||async {Json(json!({"subscription_tier_display":"Free","allow_access":true}))}))
        .route("/oauth2/token",post(move |headers:HeaderMap,Form(form):Form<HashMap<String,String>>| {
            let key=key.clone();let issuer=token_issuer.clone();
            async move {
                assert_eq!(form["client_id"],wire::CLIENT_ID);
                assert_eq!(headers["x-grok-client-version"],wire::VERSION);
                assert!(headers.get("originator").is_none());assert!(headers.get("chatgpt-account-id").is_none());
                if form.get("refresh_token").is_some_and(|v|v=="invalid") {return (StatusCode::BAD_REQUEST,Json(json!({"error":"invalid_grant","error_description":"must-not-leak-invalid-token"}))).into_response();}
                let owner=if form.get("refresh_token").is_some_and(|v|v.ends_with("bob")){"bob"}else{"alice"};
                let mut tokens=json!({"access_token":format!("access-{owner}"),"refresh_token":format!("rotated-{owner}"),"expires_in":3600});
                if form["grant_type"]=="authorization_code" {
                    assert!(!form["code_verifier"].is_empty());
                    let header=URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256","kid":"fixture"}"#);
                    let claims=json!({"iss":issuer,"aud":wire::CLIENT_ID,"sub":owner,"exp":Utc::now().timestamp()+300,"nonce":form["code"]});
                    let input=format!("{header}.{}",URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap()));
                    let sig=key.sign(&ring::rand::SystemRandom::new(),input.as_bytes()).unwrap();
                    tokens["id_token"]=format!("{input}.{}",URL_SAFE_NO_PAD.encode(sig.as_ref())).into();
                }
                Json(tokens).into_response()
            }
        }))
        .route("/v1/user",get(|headers:HeaderMap|async move {
            assert_eq!(headers["x-xai-token-auth"],wire::TOKEN_AUTH);
            assert!(headers["user-agent"].to_str().unwrap().contains("grok-pager/1.0.45"));
            let user=if headers["authorization"]=="Bearer access-bob" {"bob"}else{"alice"};
            Json(json!({"userId":user,"principalType":"User","email":format!("{user}@example.test"),"subscriptionTier":null}))
        }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let auth = GrokAuthService::new(Some(storage)).with_grok_config(GrokConfig {
        issuer: issuer.clone(),
        base_url: format!("{issuer}/v1"),
    });
    (dir, auth, server)
}

#[tokio::test]
async fn refresh_token_import_is_verified_isolated_and_reuses_only_the_same_owner() {
    let (_dir, auth, server) = fixture().await;
    let first_identity = AccountIdentity::generate();
    let first = auth
        .grok_login_rt(first_identity.clone(), None, None, "rt-alice")
        .await
        .unwrap();
    assert_eq!(first.plan_type.as_deref(), Some("Free"));
    assert_eq!(first.email.as_deref(), Some("alice@example.test"));
    let again = auth
        .grok_login_rt(AccountIdentity::generate(), None, None, "rotated-alice")
        .await
        .unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(again.installation_id, first_identity.installation_id);
    assert_eq!(again.http_fingerprint_json, first.http_fingerprint_json);
    let second = auth
        .grok_login_rt(AccountIdentity::generate(), None, None, "rt-bob")
        .await
        .unwrap();
    assert_ne!(first.id, second.id);
    assert_ne!(first.installation_id, second.installation_id);
    assert!(!Arc::ptr_eq(
        &auth.account_http(&first.id).await.unwrap(),
        &auth.account_http(&second.id).await.unwrap()
    ));
    assert!(matches!(
        auth.grok_login_rt(
            AccountIdentity::from_account(&first),
            None,
            Some(&first.id),
            "rt-bob"
        )
        .await,
        Err(AuthError::AccountMismatch)
    ));
    let fresh = auth
        .refresh_grok(&first.id, true, Some("access-alice"))
        .await
        .unwrap();
    assert_eq!(fresh.refresh_token.as_deref(), Some("rotated-alice"));
    let stored = auth
        .storage()
        .unwrap()
        .load_supplier_tokens(&first.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.auth_mode.as_deref(), Some("grok"));
    assert!(!stored.raw_auth_json.unwrap().contains("OPENAI_API_KEY"));
    let error = auth
        .grok_login_rt(AccountIdentity::generate(), None, None, "invalid")
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("must-not-leak"));
    assert_eq!(
        auth.storage().unwrap().list_accounts().await.unwrap().len(),
        2
    );
    server.abort();
}

#[tokio::test]
async fn callback_checks_exact_redirect_state_nonce_signature_and_single_use() {
    let (_dir, auth, server) = fixture().await;
    let pending = auth
        .grok_begin(AccountIdentity::generate(), None, None, false)
        .await
        .unwrap();
    let url = url::Url::parse(pending["authorize_url"].as_str().unwrap()).unwrap();
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(params["scope"], wire::SCOPE);
    assert_eq!(params["code_challenge_method"], "S256");
    let state = pending["state"].as_str().unwrap();
    assert!(matches!(
        auth.grok_complete(
            state,
            Some(&format!("{}?state=wrong&code=x", params["redirect_uri"]))
        )
        .await,
        Err(AuthError::StateMismatch)
    ));
    let wrong_nonce = format!("{}?state={state}&code=wrong-nonce", params["redirect_uri"]);
    assert!(matches!(
        auth.grok_complete(state, Some(&wrong_nonce)).await,
        Err(AuthError::InvalidIdToken)
    ));
    let callback = format!(
        "{}?state={state}&code={}",
        params["redirect_uri"], params["nonce"]
    );
    assert!(
        auth.grok_complete(state, Some(&callback))
            .await
            .unwrap()
            .is_some()
    );
    assert!(matches!(
        auth.grok_complete(state, Some(&callback)).await,
        Err(AuthError::PendingNotFound)
    ));
    let pasted = auth
        .grok_begin(AccountIdentity::generate(), None, None, false)
        .await
        .unwrap();
    let url = url::Url::parse(pasted["authorize_url"].as_str().unwrap()).unwrap();
    let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
    // Official manual login displays only the code; the exchange still verifies the nonce.
    assert!(
        auth.grok_complete(pasted["state"].as_str().unwrap(), Some(&params["nonce"]))
            .await
            .unwrap()
            .is_some()
    );
    server.abort();
}

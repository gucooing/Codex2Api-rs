use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use codex2api_storage::{Storage, TokenPurpose};
use rsa::{
    RsaPrivateKey,
    pkcs1v15::SigningKey,
    pkcs8::DecodePrivateKey,
    signature::{SignatureEncoding, Signer},
};
use serde_json::{Value, json};
use sha2::Sha256;

fn decode(token: &str, part: usize) -> Value {
    serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(token.split('.').nth(part).unwrap())
            .unwrap(),
    )
    .unwrap()
}

fn signed(key: &SigningKey<Sha256>, header: &Value, claims: &Value) -> String {
    let input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    format!(
        "{input}.{}",
        URL_SAFE_NO_PAD.encode(key.sign(input.as_bytes()).to_bytes())
    )
}

#[tokio::test]
async fn jwt_domains_purposes_algorithms_and_registered_claims_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jwt.sqlite");
    let storage = Storage::open(&path).await.unwrap();
    let now = chrono::Utc::now().timestamp();
    let input = json!({"sub":"owner","jti":"test-grant","iat":now,"exp":now+3600});
    let purposes = [
        TokenPurpose::AdminSession,
        TokenPurpose::UserSession,
        TokenPurpose::CheckoutPreview,
    ];
    let mut tokens = Vec::new();
    for purpose in purposes {
        let token = storage.sign_jwt(purpose, input.clone()).await.unwrap();
        for expected in purposes {
            assert_eq!(
                storage.verify_jwt(expected, &token).await.is_ok(),
                purpose.as_str() == expected.as_str()
            );
        }
        tokens.push(token);
    }
    assert_ne!(
        storage
            .jwt_public_key(TokenPurpose::AdminSession)
            .await
            .unwrap(),
        storage
            .jwt_public_key(TokenPurpose::UserSession)
            .await
            .unwrap()
    );
    let token = &tokens[1];
    let header = decode(token, 0);
    let claims = decode(token, 1);
    let pem: String =
        sqlx::query_scalar("SELECT private_key FROM jwt_signing_keys WHERE account_type='user'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    let signer = SigningKey::<Sha256>::new(RsaPrivateKey::from_pkcs8_pem(&pem).unwrap());
    // Even an RSA-valid signature must not negotiate an algorithm from its header.
    for algorithm in [
        json!("none"),
        json!(""),
        json!("HS256"),
        json!("RS384"),
        json!("ES256"),
        Value::Null,
    ] {
        let mut bad = header.clone();
        bad["alg"] = algorithm;
        assert!(
            storage
                .verify_jwt(TokenPurpose::UserSession, &signed(&signer, &bad, &claims))
                .await
                .is_err()
        );
    }
    for (field, value) in [
        ("account_type", json!("admin")),
        ("token_use", json!("api_access")),
        ("aud", json!(["codex2api-admin"])),
        ("iss", json!("attacker")),
        ("exp", json!(now - 1)),
        ("iat", json!(now + 600)),
        ("nbf", json!(now + 600)),
        ("nbf", json!("invalid")),
        ("sub", json!("")),
        ("jti", Value::Null),
    ] {
        let mut bad = claims.clone();
        bad[field] = value;
        assert!(
            storage
                .verify_jwt(TokenPurpose::UserSession, &signed(&signer, &header, &bad))
                .await
                .is_err(),
            "{field}"
        );
    }
    for field in ["alg", "kid", "typ"] {
        let mut bad = header.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(
            storage
                .verify_jwt(TokenPurpose::UserSession, &signed(&signer, &bad, &claims))
                .await
                .is_err()
        );
    }
    for field in [
        "exp",
        "iat",
        "aud",
        "iss",
        "sub",
        "jti",
        "account_type",
        "token_use",
    ] {
        let mut bad = claims.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(
            storage
                .verify_jwt(TokenPurpose::UserSession, &signed(&signer, &header, &bad))
                .await
                .is_err()
        );
    }
    let mut bad_header = header.clone();
    bad_header["jku"] = json!("https://attacker.invalid/jwks");
    assert!(
        storage
            .verify_jwt(
                TokenPurpose::UserSession,
                &signed(&signer, &bad_header, &claims)
            )
            .await
            .is_err()
    );
    let mut forged = claims.clone();
    forged["sub"] = json!("other-owner");
    let original_parts: Vec<_> = token.split('.').collect();
    let forged = format!(
        "{}.{}.{}",
        original_parts[0],
        URL_SAFE_NO_PAD.encode(forged.to_string()),
        original_parts[2]
    );
    assert!(
        storage
            .verify_jwt(TokenPurpose::UserSession, &forged)
            .await
            .is_err()
    );
    let admin_pem: String =
        sqlx::query_scalar("SELECT private_key FROM jwt_signing_keys WHERE account_type='admin'")
            .fetch_one(storage.pool())
            .await
            .unwrap();
    let admin_signer =
        SigningKey::<Sha256>::new(RsaPrivateKey::from_pkcs8_pem(&admin_pem).unwrap());
    assert!(
        storage
            .verify_jwt(
                TokenPurpose::UserSession,
                &signed(&admin_signer, &header, &claims)
            )
            .await
            .is_err()
    );
    for bad in ["old-session-id", "e30.e30.", "..", ""] {
        assert!(
            storage
                .verify_jwt(TokenPurpose::UserSession, bad)
                .await
                .is_err()
        );
    }
    storage.close().await;
    let reopened = Storage::open(&path).await.unwrap();
    for (purpose, token) in purposes.into_iter().zip(tokens) {
        assert!(reopened.verify_jwt(purpose, &token).await.is_ok());
    }
    reopened.close().await;
}

#[tokio::test]
async fn jwt_browser_sessions_require_live_matching_role_records() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path().join("sessions.sqlite"))
        .await
        .unwrap();
    let admin = storage
        .login_admin("admin", "admin", std::time::Duration::from_secs(3600))
        .await
        .unwrap()
        .unwrap();
    let admin_jwt = storage.admin_session_token(&admin).await.unwrap();
    let mut user = codex2api_storage::User {
        kind: codex2api_storage::UserKind::Regular,
        id: "user".into(),
        username: "admin".into(),
        password_hash: codex2api_storage::hash_password("user-password").unwrap(),
        name: "User".into(),
        email: "user@example.test".into(),
        enabled: true,
        wallet_cents: 0,
        revision: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    storage.save_user(&user, None).await.unwrap();
    let user_jwt = storage
        .create_user_session(&user, "csrf")
        .await
        .unwrap()
        .unwrap();
    assert!(
        storage
            .admin_session_from_jwt(&admin_jwt)
            .await
            .unwrap()
            .is_some()
    );
    assert!(storage.user_session(&user_jwt).await.unwrap().is_some());
    assert!(storage.user_session(&admin_jwt).await.unwrap().is_none());
    assert!(
        storage
            .admin_session_from_jwt(&user_jwt)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .admin_session_from_jwt(&admin.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .verify_admin("admin", "user-password")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        sqlx::query("UPDATE accounts SET account_type='admin' WHERE id='user'")
            .execute(storage.pool())
            .await
            .is_err()
    );
    assert!(sqlx::query("INSERT INTO users(id,name,email) SELECT account_id,'Wrong','wrong@example.test' FROM admin_users").execute(storage.pool()).await.is_err());
    user.enabled = false;
    assert!(storage.save_user(&user, Some(1)).await.unwrap());
    assert!(storage.user_session(&user_jwt).await.unwrap().is_none());
    assert!(
        storage
            .admin_session_from_jwt(&admin_jwt)
            .await
            .unwrap()
            .is_some()
    );
    storage.delete_admin_session(&admin.id).await.unwrap();
    assert!(
        storage
            .admin_session_from_jwt(&admin_jwt)
            .await
            .unwrap()
            .is_none()
    );
    storage.close().await;
}

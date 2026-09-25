//! OAuth JWT signing. Keys belong to this service and never come from suppliers.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1v15::SigningKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    signature::{SignatureEncoding, Signer},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[cfg(test)]
pub(super) async fn sign_tokens(
    storage: &codex2api_storage::Storage,
    claims: Vec<Value>,
) -> anyhow::Result<Vec<String>> {
    sign_payloads(storage, claims, false).await
}

pub(super) async fn issue_pair(
    storage: &codex2api_storage::Storage,
    claims: [Value; 2],
) -> anyhow::Result<(String, String)> {
    let mut signed = sign_payloads(storage, Vec::from(claims), true)
        .await?
        .into_iter();
    Ok((
        signed.next().expect("access token"),
        signed.next().expect("ID token"),
    ))
}

async fn sign_payloads(
    storage: &codex2api_storage::Storage,
    claims: Vec<Value>,
    pair: bool,
) -> anyhow::Result<Vec<String>> {
    let pem = match storage.oauth_jwt_private_key().await? {
        Some(pem) => pem,
        None => {
            let generated = tokio::task::spawn_blocking(|| -> anyhow::Result<String> {
                let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)?;
                Ok(key.to_pkcs8_pem(LineEnding::LF)?.to_string())
            })
            .await??;
            storage.persist_oauth_jwt_private_key(&generated).await?
        }
    };
    tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<String>> {
        let key = RsaPrivateKey::from_pkcs8_pem(&pem)?;
        let public = RsaPublicKey::from(&key).to_public_key_der()?;
        let kid = URL_SAFE_NO_PAD.encode(Sha256::digest(public.as_bytes()));
        let header = URL_SAFE_NO_PAD.encode(serde_json::to_vec(
            &json!({"alg":"RS256","typ":"JWT","kid":kid}),
        )?);
        let signer = SigningKey::<Sha256>::new(key);
        let mut access_hash = None;
        claims
            .into_iter()
            .enumerate()
            .map(|(index, mut claims)| {
                if pair && index == 1 {
                    claims["at_hash"] =
                        json!(access_hash.as_ref().expect("access token signed first"));
                }
                let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);
                let input = format!("{header}.{payload}");
                let signature = signer.try_sign(input.as_bytes())?;
                let token = format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()));
                if pair && index == 0 {
                    access_hash =
                        Some(URL_SAFE_NO_PAD.encode(&Sha256::digest(token.as_bytes())[..16]));
                }
                Ok(token)
            })
            .collect()
    })
    .await?
}

/// Verified official field sets. Business values belong only to the local account.
pub(super) fn claims(
    account: &codex2api_storage::VirtualAccount,
    device_id: &str,
    scopes: &str,
    authenticated_at_ms: i64,
    requested_at_ms: i64,
    subscription_started_at: Option<&str>,
    now: i64,
) -> [Value; 2] {
    let user = format!("user-{}", account.id);
    let organization = format!("org-{}", account.id);
    let subject_hash = format!(
        "{:x}",
        Sha256::digest(format!("oauth-sub:{}:{}", account.provider_id, account.id))
    );
    let subject = format!("auth0|{}", &subject_hash[..24]);
    let session = format!("sid_{}", device_id.replace('-', ""));
    let amr = json!(["pwd", "urn:openai:amr:password"]);
    let scope: Vec<_> = scopes.split_whitespace().collect();
    let has_email = scope.contains(&"email");
    let has_profile = scope.contains(&"profile");
    let mut profile = json!({});
    if has_email {
        profile["email"] = json!(account.email);
        profile["email_verified"] = json!(false);
    }
    if has_profile {
        profile["name"] = json!(account.name);
    }
    let access = json!({
        "aud":[codex2api_version::OAUTH_ACCESS_AUDIENCE],
        "client_id":codex2api_version::OAUTH_CLIENT_ID,
        "https://api.openai.com/auth":{
            "amr":amr,"chatgpt_account_id":account.id,"chatgpt_account_user_id":user,
            "chatgpt_compute_residency":"no_constraint","chatgpt_plan_type":account.effective_plan_at(now),
            "chatgpt_user_id":user,"localhost":true,"poid":organization,"user_id":user
        },
        "https://api.openai.com/mfa":{"required":"no"},
        "https://api.openai.com/profile":profile,
        "iss":codex2api_version::OAUTH_ISSUER,
        "pwd_auth_time":authenticated_at_ms,"scp":scope,"session_id":session,"sl":true,
        "sub":subject,"iat":now,"exp":now+codex2api_version::OAUTH_ACCESS_TOKEN_TTL,
        "jti":uuid::Uuid::new_v4().simple().to_string(),"nbf":now
    });
    let mut identity = json!({
        // Only a password has been verified locally: never claim the sample's MFA.
        "acr":"0","amr":amr,"aud":[codex2api_version::OAUTH_CLIENT_ID],
        "auth_provider":"password","auth_time":authenticated_at_ms/1000,
        "https://api.openai.com/auth":{
            "chatgpt_account_id":account.id,"chatgpt_plan_type":account.effective_plan_at(now),
            "chatgpt_subscription_active_start":subscription_started_at,
            "chatgpt_subscription_active_until":account.subscription_expires_at,
            "chatgpt_subscription_last_checked":chrono::DateTime::from_timestamp(now,0).expect("current time").to_rfc3339(),
            "chatgpt_user_id":user,"groups":[],"localhost":true,
            "organizations":[{"id":organization,"is_default":true,"role":"owner","title":account.name}],
            "user_id":user
        },
        "iss":codex2api_version::OAUTH_ISSUER,
        "rat":requested_at_ms/1000,"sid":session,"sub":subject,"iat":now,
        "exp":now+codex2api_version::OAUTH_ID_TOKEN_TTL,
        "jti":uuid::Uuid::new_v4().simple().to_string()
    });
    if has_email {
        identity["email"] = json!(account.email);
        identity["email_verified"] = json!(false);
    }
    if has_profile {
        identity["name"] = json!(account.name);
    }
    [access, identity]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::{
        pkcs1v15::{Signature, VerifyingKey},
        signature::Verifier,
    };

    #[tokio::test]
    async fn rs256_tokens_use_persistent_local_keys_and_verify_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("jwt.sqlite");
        let storage = codex2api_storage::Storage::open(&path).await.unwrap();
        let hmac = storage.oauth_signing_key().await.unwrap();
        let claims = json!({"sub":"local-user","aud":"local-client","exp":2000000000});
        let tokens = sign_tokens(&storage, vec![claims.clone()]).await.unwrap();
        let pem = storage.oauth_jwt_private_key().await.unwrap().unwrap();
        let key = RsaPrivateKey::from_pkcs8_pem(&pem).unwrap();
        let verifier = VerifyingKey::<Sha256>::new(RsaPublicKey::from(&key));
        let pieces: Vec<_> = tokens[0].split('.').collect();
        let header: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(pieces[0]).unwrap()).unwrap();
        assert_eq!(header["alg"], "RS256");
        assert_eq!(header["typ"], "JWT");
        assert!(header["kid"].as_str().is_some());
        let signature =
            Signature::try_from(URL_SAFE_NO_PAD.decode(pieces[2]).unwrap().as_slice()).unwrap();
        verifier
            .verify(
                format!("{}.{}", pieces[0], pieces[1]).as_bytes(),
                &signature,
            )
            .unwrap();
        assert!(verifier.verify(b"different payload", &signature).is_err());
        storage.close().await;
        let reopened = codex2api_storage::Storage::open(&path).await.unwrap();
        assert_eq!(reopened.oauth_signing_key().await.unwrap(), hmac);
        assert_eq!(sign_tokens(&reopened, vec![claims]).await.unwrap(), tokens);
    }
}

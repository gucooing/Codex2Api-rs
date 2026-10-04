//! Web-session and checkout JWTs. The expected purpose selects a key; untrusted JWT claims
//! and headers never select a signing domain or an algorithm.
use crate::{Result, Storage, StorageError};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    signature::{SignatureEncoding, Signer, Verifier},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;

// This is a server policy, never an algorithm negotiated from the token header.
const ALGORITHM: &str = "RS256";
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    alg: String,
    typ: String,
    kid: String,
}

/// User/admin web purposes only. Codex OAuth owns its existing signer and claims.
#[derive(Clone, Copy, Debug)]
pub enum TokenPurpose {
    AdminSession,
    UserSession,
    CheckoutPreview,
}
impl TokenPurpose {
    pub fn account_type(self) -> &'static str {
        match self {
            Self::AdminSession => "admin",
            Self::UserSession | Self::CheckoutPreview => "user",
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AdminSession => "admin_session",
            Self::UserSession => "user_session",
            Self::CheckoutPreview => "checkout_preview",
        }
    }
    fn audience(self) -> &'static str {
        match self {
            Self::AdminSession => "codex2api-admin",
            Self::UserSession => "codex2api-user",
            Self::CheckoutPreview => "codex2api-checkout",
        }
    }
    fn issuer(self) -> &'static str {
        "codex2api"
    }
    fn slot(self) -> usize {
        usize::from(!matches!(self, Self::AdminSession))
    }
}
pub(crate) struct JwtKey {
    signer: SigningKey<Sha256>,
    verifier: VerifyingKey<Sha256>,
    kid: String,
    public_pem: String,
}
impl std::fmt::Debug for JwtKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtKey")
            .field("kid", &self.kid)
            .finish_non_exhaustive()
    }
}
impl Storage {
    async fn jwt_key(&self, purpose: TokenPurpose, create: bool) -> Result<Arc<JwtKey>> {
        self.jwt_keys[purpose.slot()]
            .get_or_try_init(|| async {
                let existing: Option<String> = sqlx::query_scalar(
                    "SELECT private_key FROM jwt_signing_keys WHERE account_type=?",
                )
                .bind(purpose.account_type())
                .fetch_optional(self.pool())
                .await?;
                let pem = match existing {
                    Some(pem) => pem,
                    None if !create => return Err(StorageError::InvalidJwt),
                    None => {
                        let generated = tokio::task::spawn_blocking(|| -> Result<String> {
                            let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)
                                .map_err(|e| StorageError::Jwt(e.to_string()))?;
                            key.to_pkcs8_pem(LineEnding::LF)
                                .map(|pem| pem.to_string())
                                .map_err(|e| StorageError::Jwt(e.to_string()))
                        })
                        .await
                        .map_err(|e| StorageError::Jwt(e.to_string()))??;
                        sqlx::query("INSERT OR IGNORE INTO jwt_signing_keys(account_type,private_key) VALUES(?,?)")
                            .bind(purpose.account_type())
                            .bind(generated)
                            .execute(self.pool())
                            .await?;
                        sqlx::query_scalar("SELECT private_key FROM jwt_signing_keys WHERE account_type=?")
                            .bind(purpose.account_type())
                            .fetch_one(self.pool())
                            .await?
                    }
                };
                tokio::task::spawn_blocking(move || -> Result<Arc<JwtKey>> {
                    let private = RsaPrivateKey::from_pkcs8_pem(&pem)
                        .map_err(|e| StorageError::Jwt(e.to_string()))?;
                    let public = RsaPublicKey::from(&private);
                    let der = public.to_public_key_der()
                        .map_err(|e| StorageError::Jwt(e.to_string()))?;
                    let public_pem = public.to_public_key_pem(LineEnding::LF)
                        .map_err(|e| StorageError::Jwt(e.to_string()))?;
                    Ok(Arc::new(JwtKey {
                        signer: SigningKey::new(private),
                        verifier: VerifyingKey::new(public),
                        kid: URL_SAFE_NO_PAD.encode(Sha256::digest(der.as_bytes())),
                        public_pem,
                    }))
                })
                .await
                .map_err(|e| StorageError::Jwt(e.to_string()))?
            })
            .await
            .map(Arc::clone)
    }
    pub async fn jwt_public_key(&self, purpose: TokenPurpose) -> Result<String> {
        Ok(self.jwt_key(purpose, false).await?.public_pem.clone())
    }
    pub async fn sign_jwt(&self, purpose: TokenPurpose, mut claims: Value) -> Result<String> {
        if !claims.is_object()
            || claims["sub"].as_str().is_none_or(str::is_empty)
            || claims["jti"].as_str().is_none_or(str::is_empty)
            || claims["iat"].as_i64().is_none()
            || claims["exp"].as_i64().is_none()
        {
            return Err(StorageError::InvalidJwt);
        }
        claims["account_type"] = purpose.account_type().into();
        claims["token_use"] = purpose.as_str().into();
        claims["aud"] = json!([purpose.audience()]);
        claims["iss"] = purpose.issuer().into();
        let key = self.jwt_key(purpose, true).await?;
        tokio::task::spawn_blocking(move || {
            let header = URL_SAFE_NO_PAD
                .encode(json!({"alg":ALGORITHM,"typ":"JWT","kid":key.kid}).to_string());
            let payload = URL_SAFE_NO_PAD.encode(claims.to_string());
            let input = format!("{header}.{payload}");
            let signature = key
                .signer
                .try_sign(input.as_bytes())
                .map_err(|e| StorageError::Jwt(e.to_string()))?;
            Ok(format!(
                "{input}.{}",
                URL_SAFE_NO_PAD.encode(signature.to_bytes())
            ))
        })
        .await
        .map_err(|e| StorageError::Jwt(e.to_string()))?
    }
    pub async fn verify_jwt(&self, purpose: TokenPurpose, token: &str) -> Result<Value> {
        if token.len() > 32 * 1024 {
            return Err(StorageError::InvalidJwt);
        }
        let parts: Vec<_> = token.split('.').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
            return Err(StorageError::InvalidJwt);
        }
        let parse = |p: &str| {
            URL_SAFE_NO_PAD
                .decode(p)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .ok_or(StorageError::InvalidJwt)
        };
        let header: Header = URL_SAFE_NO_PAD
            .decode(parts[0])
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or(StorageError::InvalidJwt)?;
        if header.alg != ALGORITHM || header.typ != "JWT" {
            return Err(StorageError::InvalidJwt);
        }
        let key = self.jwt_key(purpose, false).await?;
        if header.kid != key.kid {
            return Err(StorageError::InvalidJwt);
        }
        let signature = URL_SAFE_NO_PAD
            .decode(parts[2])
            .map_err(|_| StorageError::InvalidJwt)?;
        let signature =
            Signature::try_from(signature.as_slice()).map_err(|_| StorageError::InvalidJwt)?;
        key.verifier
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .map_err(|_| StorageError::InvalidJwt)?;
        let claims = parse(parts[1])?;
        let now = chrono::Utc::now().timestamp();
        if claims["account_type"] != purpose.account_type()
            || claims["token_use"] != purpose.as_str()
            || claims["iss"] != purpose.issuer()
            || claims["aud"] != json!([purpose.audience()])
            || claims["exp"].as_i64().is_none_or(|exp| exp <= now)
            || claims["iat"].as_i64().is_none_or(|iat| iat > now)
            || claims["exp"].as_i64() <= claims["iat"].as_i64()
            || claims
                .get("nbf")
                .is_some_and(|v| v.as_i64().is_none_or(|nbf| nbf > now))
            || claims["sub"].as_str().is_none_or(str::is_empty)
            || claims["jti"].as_str().is_none_or(str::is_empty)
        {
            return Err(StorageError::InvalidJwt);
        }
        Ok(claims)
    }
}

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1v15::SigningKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding},
    signature::{SignatureEncoding, Signer},
    traits::PublicKeyParts,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

async fn key(storage: &codex2api_storage::Storage) -> anyhow::Result<String> {
    if let Some(pem) = storage.grok_oauth_key(None).await? {
        return Ok(pem);
    }
    let pem = tokio::task::spawn_blocking(|| -> anyhow::Result<String> {
        Ok(RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)?
            .to_pkcs8_pem(LineEnding::LF)?
            .to_string())
    })
    .await??;
    Ok(storage
        .grok_oauth_key(Some(&pem))
        .await?
        .expect("persisted OAuth key"))
}
fn kid(public: &RsaPublicKey) -> String {
    B64.encode(Sha256::digest(public.n().to_bytes_be()))
}
pub async fn jwks(storage: &codex2api_storage::Storage) -> anyhow::Result<Value> {
    let private = RsaPrivateKey::from_pkcs8_pem(&key(storage).await?)?;
    let public = RsaPublicKey::from(&private);
    Ok(
        json!({"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":kid(&public),"n":B64.encode(public.n().to_bytes_be()),"e":B64.encode(public.e().to_bytes_be())}]}),
    )
}
pub async fn issue(
    storage: &codex2api_storage::Storage,
    claims: [Value; 2],
) -> anyhow::Result<(String, String)> {
    let pem = key(storage).await?;
    tokio::task::spawn_blocking(move || -> anyhow::Result<(String, String)> {
        let private = RsaPrivateKey::from_pkcs8_pem(&pem)?;
        let header = B64.encode(serde_json::to_vec(
            &json!({"alg":"RS256","typ":"JWT","kid":kid(&RsaPublicKey::from(&private))}),
        )?);
        let signer = SigningKey::<Sha256>::new(private);
        let sign = |claims: Value| -> anyhow::Result<String> {
            let input = format!("{header}.{}", B64.encode(serde_json::to_vec(&claims)?));
            Ok(format!(
                "{input}.{}",
                B64.encode(signer.try_sign(input.as_bytes())?.to_bytes())
            ))
        };
        let [access, mut identity] = claims;
        let access = sign(access)?;
        identity["at_hash"] = B64.encode(&Sha256::digest(access.as_bytes())[..16]).into();
        Ok((access, sign(identity)?))
    })
    .await?
}

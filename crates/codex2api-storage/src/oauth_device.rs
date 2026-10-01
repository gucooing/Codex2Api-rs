//! Short-lived device-code authorizations; redeemed sessions use the normal device ledger.
use crate::{Result, Storage, VirtualAccount, hash_token};

pub struct DeviceAuthorization<'a> {
    pub id: &'a str,
    pub user_code: &'a str,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub verifier: &'a str,
    pub challenge: &'a str,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct DeviceAuthorizationCode {
    pub authorization_code: String,
    pub code_verifier: String,
    pub code_challenge: String,
}

pub enum DeviceAuthorizationPoll {
    Pending,
    Expired,
    Authorized(DeviceAuthorizationCode),
}

pub struct DeviceAuthorizationApproval<'a> {
    pub id: &'a str,
    pub cookie: &'a str,
    pub csrf: &'a str,
    pub account: &'a VirtualAccount,
    pub code: &'a str,
    pub scopes: &'a str,
}

impl Storage {
    pub async fn create_device_authorization(
        &self,
        request: DeviceAuthorization<'_>,
    ) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM virtual_device_authorizations WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        // Bound anonymous pending state without retaining plaintext user codes.
        let inserted = sqlx::query("INSERT INTO virtual_device_authorizations(id_hash,user_code_hash,client_id,redirect_uri,code_verifier,code_challenge,created_at,expires_at) SELECT ?,?,?,?,?,?,?,? WHERE (SELECT count(*) FROM virtual_device_authorizations)<10000")
            .bind(hash_token(request.id)).bind(hash_token(request.user_code))
            .bind(request.client_id).bind(request.redirect_uri).bind(request.verifier)
            .bind(request.challenge).bind(now).bind(now+900).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(inserted.rows_affected() == 1)
    }

    pub async fn poll_device_authorization(
        &self,
        id: &str,
        user_code: &str,
    ) -> Result<DeviceAuthorizationPoll> {
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool().begin().await?;
        let row: Option<(i64,i64,Option<String>,String,String)> = sqlx::query_as("SELECT expires_at,last_poll_at,authorization_code,code_verifier,code_challenge FROM virtual_device_authorizations WHERE id_hash=? AND user_code_hash=?")
            .bind(hash_token(id)).bind(hash_token(user_code)).fetch_optional(&mut *tx).await?;
        let Some((expires, last_poll, code, verifier, challenge)) = row else {
            return Ok(DeviceAuthorizationPoll::Expired);
        };
        if expires <= now {
            return Ok(DeviceAuthorizationPoll::Expired);
        }
        if now - last_poll < 5 {
            return Ok(DeviceAuthorizationPoll::Pending);
        }
        sqlx::query("UPDATE virtual_device_authorizations SET last_poll_at=? WHERE id_hash=?")
            .bind(now)
            .bind(hash_token(id))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(match code {
            Some(authorization_code) => {
                DeviceAuthorizationPoll::Authorized(DeviceAuthorizationCode {
                    authorization_code,
                    code_verifier: verifier,
                    code_challenge: challenge,
                })
            }
            None => DeviceAuthorizationPoll::Pending,
        })
    }

    pub async fn approve_device_authorization(
        &self,
        user_code: &str,
        request: DeviceAuthorizationApproval<'_>,
    ) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool().begin().await?;
        let flow: Option<String> = sqlx::query_scalar("DELETE FROM oauth_browser_flows WHERE id=? AND cookie_hash=? AND csrf_hash=? AND expires_at>? AND request_json='device' RETURNING id")
            .bind(request.id).bind(hash_token(request.cookie)).bind(hash_token(request.csrf))
            .bind(now).fetch_optional(&mut *tx).await?;
        if flow.is_none() {
            return Ok(false);
        }
        let row: Option<(String,String,String,i64)> = sqlx::query_as("UPDATE virtual_device_authorizations SET authorization_code=?,virtual_account_id=?,expires_at=min(expires_at,?) WHERE user_code_hash=? AND authorization_code IS NULL AND expires_at>? AND EXISTS(SELECT 1 FROM virtual_accounts WHERE id=? AND password_hash=? AND enabled=1 AND provider_id='chatgpt') RETURNING client_id,redirect_uri,code_challenge,created_at")
            .bind(request.code).bind(&request.account.id).bind(now+120).bind(hash_token(user_code))
            .bind(now).bind(&request.account.id).bind(&request.account.password_hash).fetch_optional(&mut *tx).await?;
        let Some((client, redirect, challenge, created)) = row else {
            return Ok(false);
        };
        sqlx::query("INSERT INTO virtual_authorization_codes(code_hash,virtual_account_id,client_id,redirect_uri,code_challenge,expires_at,provider_id,scopes,authenticated_at_ms,requested_at_ms) VALUES(?,?,?,?,?,?,'chatgpt',?,?,?)")
            .bind(hash_token(request.code)).bind(&request.account.id).bind(client).bind(redirect)
            .bind(challenge).bind(now+120).bind(request.scopes).bind(now*1000).bind(created*1000)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
}

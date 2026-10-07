//! Short-lived device-code authorizations; redeemed sessions use the normal device ledger.
use crate::{PlatformAccount, Result, Storage, hash_token};

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
pub enum GrokDevicePoll {
    Pending,
    SlowDown,
    Expired,
    Authorized(DeviceAuthorizationCode),
}

pub struct DeviceAuthorizationApproval<'a> {
    pub id: &'a str,
    pub cookie: &'a str,
    pub csrf: &'a str,
    pub account: &'a PlatformAccount,
    pub code: &'a str,
    pub scopes: &'a str,
}

impl Storage {
    pub async fn create_device_authorization(
        &self,
        request: DeviceAuthorization<'_>,
    ) -> Result<bool> {
        self.create_device_authorization_scoped(request, None).await
    }
    pub async fn create_device_authorization_scoped(
        &self,
        request: DeviceAuthorization<'_>,
        scopes: Option<&str>,
    ) -> Result<bool> {
        let provider = if request.client_id == codex2api_version::grok::CLIENT_ID {
            codex2api_core::GROK
        } else if request.client_id == codex2api_version::OAUTH_CLIENT_ID {
            codex2api_core::CHATGPT
        } else {
            return Ok(false);
        };
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM virtual_device_authorizations WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        // Bound anonymous pending state without retaining plaintext user codes.
        let inserted = sqlx::query("INSERT INTO virtual_device_authorizations(id_hash,user_code_hash,client_id,redirect_uri,code_verifier,code_challenge,created_at,expires_at,provider_id,requested_scopes) SELECT ?,?,?,?,?,?,?,?,?,? WHERE (SELECT count(*) FROM virtual_device_authorizations)<10000")
            .bind(hash_token(request.id)).bind(hash_token(request.user_code))
            .bind(request.client_id).bind(request.redirect_uri).bind(request.verifier)
            .bind(request.challenge).bind(now).bind(now+900).bind(provider).bind(scopes).execute(&mut *tx).await?;
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
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        if !crate::users::confirmed_browser_identity(&mut tx, request.id, request.account).await? {
            return Ok(false);
        }
        let flow_kind = if request.account.provider_id == codex2api_core::GROK {
            "device-grok"
        } else {
            "device"
        };
        let flow: Option<String> = sqlx::query_scalar("DELETE FROM oauth_browser_flows WHERE id=? AND cookie_hash=? AND csrf_hash=? AND expires_at>? AND request_json=? RETURNING id")
            .bind(request.id).bind(hash_token(request.cookie)).bind(hash_token(request.csrf))
            .bind(now).bind(flow_kind).fetch_optional(&mut *tx).await?;
        if flow.is_none() {
            return Ok(false);
        }
        let row: Option<(String,String,String,i64,Option<String>)> = sqlx::query_as("UPDATE virtual_device_authorizations SET authorization_code=?,virtual_account_id=?,expires_at=min(expires_at,?) WHERE user_code_hash=? AND authorization_code IS NULL AND expires_at>? AND provider_id=? AND EXISTS(SELECT 1 FROM platform_principals WHERE id=? AND password_hash=? AND enabled=1 AND provider_id=?) RETURNING client_id,redirect_uri,code_challenge,created_at,requested_scopes")
            .bind(request.code).bind(&request.account.id).bind(now+120).bind(hash_token(user_code))
            .bind(now).bind(&request.account.provider_id).bind(&request.account.id).bind(&request.account.password_hash).bind(&request.account.provider_id).fetch_optional(&mut *tx).await?;
        let Some((client, redirect, challenge, created, scopes)) = row else {
            return Ok(false);
        };
        let scopes = scopes.as_deref().unwrap_or(request.scopes);
        if !scopes
            .split_whitespace()
            .all(|s| request.scopes.split_whitespace().any(|a| a == s))
        {
            return Ok(false);
        }
        sqlx::query("INSERT INTO virtual_authorization_codes(code_hash,virtual_account_id,client_id,redirect_uri,code_challenge,expires_at,provider_id,scopes,authenticated_at_ms,requested_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(hash_token(request.code)).bind(&request.account.id).bind(client).bind(redirect)
            .bind(challenge).bind(now+120).bind(&request.account.provider_id).bind(scopes).bind(now*1000).bind(created*1000)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn poll_grok_device(&self, id: &str) -> Result<GrokDevicePoll> {
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row: Option<(i64,i64,i64,Option<String>,String,String)> = sqlx::query_as("SELECT expires_at,last_poll_at,poll_interval,authorization_code,code_verifier,code_challenge FROM virtual_device_authorizations WHERE id_hash=? AND provider_id='grok'")
            .bind(hash_token(id)).fetch_optional(&mut *tx).await?;
        let Some((expiry, last, interval, code, verifier, challenge)) = row else {
            return Ok(GrokDevicePoll::Expired);
        };
        if expiry <= now {
            return Ok(GrokDevicePoll::Expired);
        }
        let fast = now - last < interval;
        sqlx::query("UPDATE virtual_device_authorizations SET last_poll_at=?,poll_interval=poll_interval+? WHERE id_hash=? AND provider_id='grok'")
            .bind(now).bind(if fast {5} else {0}).bind(hash_token(id)).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(if fast {
            GrokDevicePoll::SlowDown
        } else if let Some(authorization_code) = code {
            GrokDevicePoll::Authorized(DeviceAuthorizationCode {
                authorization_code,
                code_verifier: verifier,
                code_challenge: challenge,
            })
        } else {
            GrokDevicePoll::Pending
        })
    }
}

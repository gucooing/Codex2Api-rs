use crate::{Result, Storage, hash_token};

pub struct BrowserAuthorization<'a> {
    pub id: &'a str,
    pub cookie: &'a str,
    pub csrf: &'a str,
    pub account: &'a crate::VirtualAccount,
    pub code: &'a str,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub challenge: &'a str,
    pub scopes: &'a str,
}

pub struct CodeRedemption<'a> {
    pub provider: &'a str,
    pub code: &'a str,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub challenge: &'a str,
    pub refresh: &'a str,
    pub device: &'a crate::OAuthDeviceIdentity,
}

impl Storage {
    pub async fn create_oauth_browser_flow(
        &self,
        id: &str,
        cookie: &str,
        csrf: &str,
        request: &str,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM oauth_browser_flows WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM virtual_authorization_codes WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO oauth_browser_flows(id,cookie_hash,csrf_hash,request_json,expires_at) VALUES(?,?,?,?,?)")
            .bind(id).bind(hash_token(cookie)).bind(hash_token(csrf)).bind(request).bind(now+600).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn oauth_browser_flow(
        &self,
        id: &str,
        cookie: &str,
        csrf: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT request_json FROM oauth_browser_flows WHERE id=? AND cookie_hash=? AND csrf_hash=? AND expires_at>?")
            .bind(id).bind(hash_token(cookie)).bind(hash_token(csrf)).bind(chrono::Utc::now().timestamp()).fetch_optional(self.pool()).await?)
    }

    pub async fn authorize_oauth_browser_flow(
        &self,
        request: BrowserAuthorization<'_>,
    ) -> Result<bool> {
        let BrowserAuthorization {
            id,
            cookie,
            csrf,
            account,
            code,
            client_id,
            redirect_uri,
            challenge,
            scopes,
        } = request;
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.pool().begin().await?;
        let removed=sqlx::query("DELETE FROM oauth_browser_flows WHERE id=? AND cookie_hash=? AND csrf_hash=? AND expires_at>?")
            .bind(id).bind(hash_token(cookie)).bind(hash_token(csrf)).bind(now).execute(&mut *tx).await?;
        if removed.rows_affected() != 1 {
            return Ok(false);
        }
        let inserted=sqlx::query("INSERT INTO virtual_authorization_codes(code_hash,virtual_account_id,client_id,redirect_uri,code_challenge,expires_at,provider_id,scopes)
            SELECT ?,v.id,?,?,?,?,v.provider_id,? FROM virtual_accounts v
            WHERE v.id=? AND v.password_hash=? AND v.enabled=1")
            .bind(hash_token(code)).bind(client_id).bind(redirect_uri).bind(challenge).bind(now+120).bind(scopes).bind(&account.id).bind(&account.password_hash).execute(&mut *tx).await?;
        if inserted.rows_affected() != 1 {
            return Ok(false);
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Atomic single-use redemption bound to client, exact redirect URI and S256 proof.
    pub async fn redeem_oauth_code(
        &self,
        request: CodeRedemption<'_>,
    ) -> Result<Option<(crate::VirtualAccount, String)>> {
        let CodeRedemption {
            provider,
            code,
            client_id,
            redirect_uri,
            challenge,
            refresh,
            device,
        } = request;
        let mut tx = self.pool().begin().await?;
        let token: Option<(String,String,String)>=sqlx::query_as("DELETE FROM virtual_authorization_codes WHERE code_hash=? AND client_id=? AND redirect_uri=? AND code_challenge=? AND expires_at>? AND provider_id=? RETURNING virtual_account_id,provider_id,scopes")
            .bind(hash_token(code)).bind(client_id).bind(redirect_uri).bind(challenge).bind(chrono::Utc::now().timestamp()).bind(provider).fetch_optional(&mut *tx).await?;
        let Some((owner, provider, scopes)) = token else {
            return Ok(None);
        };
        let account: Option<crate::VirtualAccount> =
            sqlx::query_as("SELECT * FROM virtual_accounts WHERE id=? AND enabled=1")
                .bind(&owner)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(account) = account else {
            return Ok(None);
        };
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        // Code consumption and session creation must serialize with password changes and disable.
        sqlx::query("INSERT INTO virtual_devices(id,virtual_account_id,refresh_hash,installation_id,user_agent,created_at,last_login_at,provider_id,scopes) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(&owner).bind(hash_token(refresh)).bind(&device.installation_id).bind(&device.user_agent).bind(&now).bind(&now).bind(provider).bind(scopes).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some((account, id)))
    }
}

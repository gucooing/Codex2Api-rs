use crate::{Result, Storage, VirtualAccess, hash_token};
use serde::Deserialize;
use sqlx::FromRow;

#[derive(Deserialize)]
pub struct RemoteServerRegistration {
    pub name: String,
    pub os: String,
    pub arch: String,
    pub app_server_version: String,
    pub installation_id: String,
}

#[derive(FromRow, serde::Serialize)]
pub struct RemoteServer {
    pub id: String,
    pub environment_id: String,
    pub virtual_account_id: String,
    pub device_id: String,
    pub installation_id: String,
    pub name: String,
    pub os: String,
    pub arch: String,
    pub app_server_version: String,
    pub expires_at: i64,
    pub connected_until_ms: i64,
    pub last_seen_at_ms: i64,
}

impl Storage {
    pub async fn enroll_remote_server(
        &self,
        access: &VirtualAccess,
        input: &RemoteServerRegistration,
        token: &str,
        expires: i64,
    ) -> Result<RemoteServer> {
        let now = chrono::Utc::now().timestamp_millis();
        Ok(sqlx::query_as("INSERT INTO virtual_remote_servers(id,environment_id,virtual_account_id,device_id,installation_id,name,os,arch,app_server_version,token_hash,expires_at,created_at_ms,last_seen_at_ms) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(virtual_account_id,installation_id) DO UPDATE SET device_id=excluded.device_id,name=excluded.name,os=excluded.os,arch=excluded.arch,app_server_version=excluded.app_server_version,token_hash=excluded.token_hash,expires_at=excluded.expires_at,last_seen_at_ms=excluded.last_seen_at_ms RETURNING *")
            .bind(format!("srv_{}",uuid::Uuid::new_v4())).bind(format!("env_{}",uuid::Uuid::new_v4())).bind(&access.virtual_account_id).bind(&access.device_id).bind(&input.installation_id).bind(&input.name).bind(&input.os).bind(&input.arch).bind(&input.app_server_version).bind(hash_token(token)).bind(expires).bind(now).bind(now).fetch_one(self.pool()).await?)
    }

    pub async fn refresh_remote_server(
        &self,
        access: &VirtualAccess,
        server: &str,
        installation: &str,
        token: &str,
        expires: i64,
    ) -> Result<Option<RemoteServer>> {
        Ok(sqlx::query_as("UPDATE virtual_remote_servers SET token_hash=?,expires_at=?,last_seen_at_ms=? WHERE id=? AND virtual_account_id=? AND device_id=? AND installation_id=? RETURNING *")
            .bind(hash_token(token)).bind(expires).bind(chrono::Utc::now().timestamp_millis()).bind(server).bind(&access.virtual_account_id).bind(&access.device_id).bind(installation).fetch_optional(self.pool()).await?)
    }

    pub async fn remote_server_for_token(&self, token: &str) -> Result<Option<RemoteServer>> {
        Ok(sqlx::query_as("SELECT s.* FROM virtual_remote_servers s JOIN virtual_accounts v ON v.id=s.virtual_account_id JOIN virtual_devices d ON d.id=s.device_id AND d.virtual_account_id=v.id WHERE s.token_hash=? AND s.expires_at>? AND v.enabled=1")
            .bind(hash_token(token)).bind(chrono::Utc::now().timestamp()).fetch_optional(self.pool()).await?)
    }

    pub async fn remote_servers(&self, owner: &str) -> Result<Vec<RemoteServer>> {
        Ok(sqlx::query_as("SELECT * FROM virtual_remote_servers WHERE virtual_account_id=? ORDER BY last_seen_at_ms DESC,id").bind(owner).fetch_all(self.pool()).await?)
    }

    pub async fn connect_remote_server(&self, id: &str, connection: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE virtual_remote_servers SET connection_id=?,connected_until_ms=?,last_seen_at_ms=? WHERE id=?")
            .bind(connection).bind(now+30000).bind(now).bind(id).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn touch_remote_server(&self, id: &str, connection: &str) -> Result<bool> {
        let now = chrono::Utc::now().timestamp_millis();
        Ok(sqlx::query("UPDATE virtual_remote_servers SET connected_until_ms=?,last_seen_at_ms=? WHERE id=? AND connection_id=?")
            .bind(now+30000).bind(now).bind(id).bind(connection).execute(self.pool()).await?.rows_affected()==1)
    }

    pub async fn disconnect_remote_server(&self, id: &str, connection: &str) -> Result<()> {
        sqlx::query("UPDATE virtual_remote_servers SET connection_id=NULL,connected_until_ms=0 WHERE id=? AND connection_id=?")
            .bind(id).bind(connection).execute(self.pool()).await?;
        Ok(())
    }
}

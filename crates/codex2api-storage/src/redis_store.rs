//! One connection manager and namespace for all ephemeral business state.
use crate::{Result, StorageError};
use redis::{FromRedisValue, aio::ConnectionManager};
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct RedisStore {
    connection: ConnectionManager,
    prefix: String,
}

impl RedisStore {
    pub async fn connect(identity: &str) -> Result<Self> {
        use sha2::{Digest, Sha256};
        let url = std::env::var("CODEX2API_REDIS_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .ok_or(StorageError::RedisNotConfigured)?;
        let client = redis::Client::open(url).map_err(|_| StorageError::Runtime)?;
        let connection =
            tokio::time::timeout(Duration::from_secs(5), ConnectionManager::new(client))
                .await
                .map_err(|_| StorageError::Runtime)?
                .map_err(|_| StorageError::Runtime)?;
        let namespace = format!("{:x}", Sha256::digest(identity.as_bytes()));
        let prefix = std::env::var("CODEX2API_REDIS_PREFIX")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| format!("codex2api:{}", &namespace[..24]));
        Ok(Self { connection, prefix })
    }

    pub fn key(&self, key: &str) -> String {
        format!("{}:{key}", self.prefix)
    }

    pub async fn command<T: FromRedisValue>(&self, command: &redis::Cmd) -> Result<T> {
        tokio::time::timeout(
            Duration::from_secs(2),
            command.query_async(&mut self.connection.clone()),
        )
        .await
        .map_err(|_| StorageError::Runtime)?
        .map_err(|_| StorageError::Runtime)
    }

    pub async fn pipeline<T: FromRedisValue>(&self, pipeline: &redis::Pipeline) -> Result<T> {
        tokio::time::timeout(
            Duration::from_secs(2),
            pipeline.query_async(&mut self.connection.clone()),
        )
        .await
        .map_err(|_| StorageError::Runtime)?
        .map_err(|_| StorageError::Runtime)
    }

    pub async fn script<T: FromRedisValue>(
        &self,
        script: &redis::ScriptInvocation<'_>,
    ) -> Result<T> {
        tokio::time::timeout(
            Duration::from_secs(2),
            script.invoke_async(&mut self.connection.clone()),
        )
        .await
        .map_err(|_| StorageError::Runtime)?
        .map_err(|_| StorageError::Runtime)
    }
}

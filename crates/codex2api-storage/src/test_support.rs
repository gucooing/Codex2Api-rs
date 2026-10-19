//! Real PostgreSQL fixtures. Each temporary directory owns isolated test schemas.
use crate::{Result, Storage};
use sha2::{Digest, Sha256};
use sqlx::{Connection, postgres::PgConnectOptions};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

static SCHEMAS: LazyLock<Mutex<HashMap<PathBuf, Vec<String>>>> = LazyLock::new(Mutex::default);

pub struct TestDirectory(tempfile::TempDir);

pub fn tempdir() -> std::io::Result<TestDirectory> {
    let directory = tempfile::tempdir()?;
    SCHEMAS
        .lock()
        .unwrap()
        .insert(directory.path().to_owned(), Vec::new());
    Ok(TestDirectory(directory))
}

impl TestDirectory {
    pub fn path(&self) -> &Path {
        self.0.path()
    }
}

fn options() -> PgConnectOptions {
    std::env::var("CODEX2API_TEST_DATABASE_URL")
        .expect("CODEX2API_TEST_DATABASE_URL must point to a dedicated PostgreSQL test database")
        .parse()
        .expect("valid PostgreSQL test URL")
}

pub async fn open(identity: impl AsRef<Path>) -> Result<Storage> {
    let identity = identity.as_ref();
    let schema = format!(
        "test_{:x}",
        Sha256::digest(identity.to_string_lossy().as_bytes())
    );
    let schema = schema[..48].to_owned();
    let mut connection = sqlx::PgConnection::connect_with(&options()).await?;
    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {schema}"))
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    for (directory, schemas) in SCHEMAS.lock().unwrap().iter_mut() {
        if identity.starts_with(directory) && !schemas.contains(&schema) {
            schemas.push(schema.clone());
        }
    }
    let storage =
        Storage::connect_with(options().options([("search_path", schema.as_str())])).await?;
    // The process registers built-in models at startup; fixtures mirror that step.
    storage
        .sync_supported_models(&codex2api_common::builtin_models())
        .await?;
    Ok(storage)
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let schemas = SCHEMAS
            .lock()
            .unwrap()
            .remove(self.path())
            .unwrap_or_default();
        if schemas.is_empty() {
            return;
        }
        // A separate runtime also works after the test's Tokio runtime has stopped.
        let _ = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let Ok(mut connection) = sqlx::PgConnection::connect_with(&options()).await else {
                    return;
                };
                let _ = sqlx::query("SET lock_timeout = '2s'")
                    .execute(&mut connection)
                    .await;
                for schema in schemas {
                    let _ = sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
                        .execute(&mut connection)
                        .await;
                }
            });
        })
        .join();
    }
}

/// Expose the actual ephemeral namespace to tests that adjust Redis server time.
pub fn redis_key(storage: &Storage, suffix: &str) -> String {
    storage.runtime.redis.key(suffix)
}
